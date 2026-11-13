//! Job nodes: a run handing work to the job queue.

use super::*;

fn job_graph(job: &str) -> Value {
    json!({
        "nodes": [{
            "id": "job",
            "name": "Job",
            "kind": "job",
            "ports": [port("result", "output", "json")],
            "config": { "job_definition_id": job, "execution_pool": "mini" },
        }],
        "hyperedges": [
            edge("job-out", &at("job", "result", "json"), &field("result", "json")),
        ],
        "transition_policy": { "mode": "static", "cycles_allowed": false },
    })
}

async fn job_definition(stack: &Stack) -> String {
    let id = unique("job");
    let definition = JobDefinition::new(
        JobDefinitionId::new(id.clone()).expect("a valid id"),
        "A job",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "job.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("a valid definition");
    stack
        .store
        .upsert_job_definition_in_project(&definition, &stack.tenant, &stack.project)
        .await
        .expect("stored");
    id
}

/// Waits for the job run a node created, then finishes it the way a worker would.
async fn finish_job_when_it_appears(
    store: PostgresStore,
    tenant: String,
    project: String,
    run: String,
    status: &'static str,
) {
    for _ in 0..400 {
        if let Some(key) = claimed_key(&store, &tenant, &project, &run).await {
            let id = JobRunId::new(derived_id("agent_job_", &key)).expect("a valid id");
            if store.find_by_id(&id).await.expect("reads").is_some() {
                // Standing in for the job worker, which is not what this test is about.
                let pool = sqlx::PgPool::connect(&database_url())
                    .await
                    .expect("connects");
                sqlx::query("UPDATE job_runs SET status = $2 WHERE id = $1")
                    .bind(id.as_str())
                    .bind(status)
                    .execute(&pool)
                    .await
                    .expect("updates");
                pool.close().await;
                return;
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("the job run never appeared");
}

#[tokio::test]
async fn a_job_node_runs_its_job_once_and_reports_how_it_came_out() {
    let stack = Stack::new(None).await;
    let job = job_definition(&stack).await;
    let agent = stack
        .agent(job_graph(&job), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;

    let waiter = tokio::spawn(finish_job_when_it_appears(
        stack.store.clone(),
        stack.tenant.clone(),
        stack.project.clone(),
        id.clone(),
        "succeeded",
    ));
    stack.run_to_rest().await;
    waiter.await.expect("the job finished");

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");
    assert_eq!(run["state"]["result"]["status"], "succeeded");
    assert_eq!(run["execution"]["usage"]["effects"], 1);

    let kinds = stack.event_kinds(&id).await;
    let claimed = kinds
        .iter()
        .filter(|kind| *kind == "effect_claimed")
        .count();
    let finalized = kinds
        .iter()
        .filter(|kind| *kind == "effect_finalized")
        .count();
    assert_eq!((claimed, finalized), (1, 1));
}

#[tokio::test]
async fn a_job_that_fails_fails_the_node_with_the_effect_still_on_record() {
    let stack = Stack::new(None).await;
    let job = job_definition(&stack).await;
    let agent = stack
        .agent(job_graph(&job), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;

    let waiter = tokio::spawn(finish_job_when_it_appears(
        stack.store.clone(),
        stack.tenant.clone(),
        stack.project.clone(),
        id.clone(),
        "failed",
    ));
    stack.run_to_rest().await;
    waiter.await.expect("the job finished");

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    let kinds = stack.event_kinds(&id).await;
    assert!(
        kinds.contains(&"effect_finalized".to_string()),
        "the job ran, so its effect is not abandoned: {kinds:?}"
    );
    assert_decisions_replay(&stack, &id).await;
}

#[tokio::test]
async fn a_job_in_another_project_is_refused_rather_than_run() {
    let stack = Stack::new(None).await;
    let foreign = unique("foreign_job");
    let definition = JobDefinition::new(
        JobDefinitionId::new(foreign.clone()).expect("a valid id"),
        "Not yours",
        "python:3.12-slim",
        vec!["python".to_string()],
        Vec::new(),
        "job.py",
        "{}",
        capsulet_core::RetryPolicy::no_retry(),
    )
    .expect("a valid definition");
    stack
        .store
        .upsert_job_definition_in_project(&definition, &stack.tenant, "another-project")
        .await
        .expect("stored");

    let agent = stack
        .agent(job_graph(&foreign), generous(), &["validator_pass"])
        .await;
    let id = stack.start(&agent, json!({})).await;
    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("policy_denial"),
        "{run}"
    );
    let key = claimed_key(&stack.store, &stack.tenant, &stack.project, &id)
        .await
        .expect("the attempt was claimed");
    assert!(
        stack
            .store
            .find_by_id(&JobRunId::new(derived_id("agent_job_", &key)).expect("a valid id"))
            .await
            .expect("reads")
            .is_none(),
        "no job run may exist for a definition this project does not own"
    );
}
