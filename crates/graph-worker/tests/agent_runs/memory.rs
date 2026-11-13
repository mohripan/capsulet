//! Memory writes: what an agent may propose, and what it may not.

use super::*;

#[tokio::test]
async fn an_agent_can_only_propose_a_claim_never_activate_one() {
    let stack = Stack::new(None).await;
    let subject = unique("acme");
    stack
        .store
        .upsert_memory_entity(
            &Entity::new(
                EntityId::new(subject.clone()).expect("a valid id"),
                MemoryScope::new(stack.tenant.clone(), stack.project.clone()).expect("a scope"),
                "organization",
                "Acme",
                Vec::new(),
            )
            .expect("a valid entity"),
        )
        .await
        .expect("stored");
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(
            &agent,
            json!({ "claim": {
                "subject_id": subject,
                "predicate": "renewed",
                "object": "2026-03-01",
                "evidence_ids": ["evidence-1"],
                "observed_at": "2026-11-08T00:00:00Z",
                "status": "active",
            }}),
        )
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "succeeded", "{run}");

    let key = claimed_key(&stack.store, &stack.tenant, &stack.project, &id)
        .await
        .expect("the attempt was claimed");
    let claim_id = ClaimId::new(derived_id("agent_claim_", &key)).expect("a valid id");
    let claim = stack
        .store
        .find_memory_claim(&claim_id)
        .await
        .expect("reads")
        .expect("the claim was proposed");
    assert_eq!(
        claim.status().to_string(),
        "candidate",
        "an agent's output is a candidate until somebody reviews it"
    );
    assert_eq!(claim.scope().project_id(), stack.project);
}

#[tokio::test]
async fn a_claim_that_cannot_be_built_is_refused_and_writes_nothing() {
    let stack = Stack::new(None).await;
    let agent = stack
        .agent(memory_write_graph(), generous(), &["validator_pass"])
        .await;
    let id = stack
        .start(&agent, json!({ "claim": { "predicate": "renewed" } }))
        .await;

    stack.run_to_rest().await;

    let run = stack.run(&id).await;
    assert_eq!(run["status"], "failed", "{run}");
    assert!(
        run["execution"]["detail"]
            .as_str()
            .expect("a reason")
            .contains("a claim to write needs"),
        "{run}"
    );
    let kinds = stack.event_kinds(&id).await;
    assert!(kinds.contains(&"effect_abandoned".to_string()), "{kinds:?}");
}
