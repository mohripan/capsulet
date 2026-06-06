"use client";

import { FormEvent, useEffect, useState } from "react";
import { GitBranch, Plus, RefreshCw, Send, Workflow as WorkflowIcon } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import {
  ExecutionPool,
  JobDefinition,
  Workflow,
  createWorkflow,
  getErrorMessage,
  listExecutionPools,
  listJobDefinitions,
  listWorkflows
} from "../lib/api";

export default function WorkflowsPage() {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [definitions, setDefinitions] = useState<JobDefinition[]>([]);
  const [pools, setPools] = useState<ExecutionPool[]>([]);
  const [name, setName] = useState("Hourly email workflow");
  const [description, setDescription] = useState("Run user-authored jobs in sequence.");
  const [firstJob, setFirstJob] = useState("");
  const [secondJob, setSecondJob] = useState("");
  const [pool, setPool] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const [workflowResponse, definitionResponse, poolResponse] = await Promise.all([
        listWorkflows(),
        listJobDefinitions(),
        listExecutionPools()
      ]);
      setWorkflows(workflowResponse.workflows);
      setDefinitions(definitionResponse.job_definitions);
      setPools(poolResponse.execution_pools);
      setFirstJob((current) => current || definitionResponse.job_definitions[0]?.id || "");
      setSecondJob((current) => current || definitionResponse.job_definitions[1]?.id || definitionResponse.job_definitions[0]?.id || "");
      setPool(
        (current) =>
          current ||
          poolResponse.execution_pools.find((item) => item.is_default)?.name ||
          poolResponse.execution_pools[0]?.name ||
          ""
      );
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);
    try {
      await createWorkflow({
        name,
        description,
        steps: [
          { name: "Step 1", job_definition_id: firstJob, execution_pool: pool },
          { name: "Step 2", job_definition_id: secondJob, execution_pool: pool }
        ]
      });
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Workflow authoring"
        title="Create linear workflows"
        description="Compose reusable job definitions into ordered steps. Branching and dependency graphs come later."
      />

      <section className="contentGrid">
        <section className="panel span5">
          <PanelTitle icon={Plus} title="New Workflow" action="Linear MVP" />
          <form className="formStack" onSubmit={submit}>
            <label>
              <span>Name</span>
              <input value={name} onChange={(event) => setName(event.target.value)} />
            </label>
            <label>
              <span>Description</span>
              <input value={description} onChange={(event) => setDescription(event.target.value)} />
            </label>
            <label>
              <span>Step 1 job definition</span>
              <select value={firstJob} onChange={(event) => setFirstJob(event.target.value)}>
                {definitions.map((definition) => (
                  <option value={definition.id} key={definition.id}>
                    {definition.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Step 2 job definition</span>
              <select value={secondJob} onChange={(event) => setSecondJob(event.target.value)}>
                {definitions.map((definition) => (
                  <option value={definition.id} key={definition.id}>
                    {definition.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Execution pool</span>
              <select value={pool} onChange={(event) => setPool(event.target.value)}>
                {pools.map((item) => (
                  <option value={item.name} key={item.name}>
                    {item.name}
                  </option>
                ))}
              </select>
            </label>
            <button className="primaryAction inlineAction" disabled={isSubmitting || !firstJob || !secondJob || !pool}>
              <Send size={16} aria-hidden="true" />
              {isSubmitting ? "Creating" : "Create workflow"}
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={WorkflowIcon} title="Workflow Definitions" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          <div className="workflowList">
            {!isLoading && workflows.length === 0 ? (
              <div className="emptyState">No workflows yet. Create job definitions first, then compose them here.</div>
            ) : null}
            {workflows.map((workflow) => (
              <article className="workflowCard" key={workflow.id}>
                <div className="poolTop">
                  <div>
                    <h2>{workflow.name}</h2>
                    <p>{workflow.id} / {workflow.status}</p>
                  </div>
                  <span>{workflow.steps.length} steps</span>
                </div>
                <div className="stepRail">
                  {workflow.steps.map((step) => (
                    <div className="stepItem" key={step.id}>
                      <span>{step.position}</span>
                      <strong title={step.job_definition_id}>{step.name}: {step.job_definition_id}</strong>
                    </div>
                  ))}
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={GitBranch} title="Workflow Scope" action="MVP" />
          <div className="wideNotice">
            <strong>Current workflows are linear.</strong>
            <span>Schedules, webhooks, dependency triggers, branching, and fan-out/fan-in are intentionally deferred.</span>
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
