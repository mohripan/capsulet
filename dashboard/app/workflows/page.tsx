"use client";

import { FormEvent, Fragment, useEffect, useState } from "react";
import { ChevronLeft, ChevronRight, FileCode2, GitBranch, Plus, RefreshCw, Send, Workflow as WorkflowIcon } from "lucide-react";
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
  const [selectedWorkflowIndex, setSelectedWorkflowIndex] = useState(0);
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

  useEffect(() => {
    if (selectedWorkflowIndex > Math.max(0, workflows.length - 1)) {
      setSelectedWorkflowIndex(Math.max(0, workflows.length - 1));
    }
  }, [selectedWorkflowIndex, workflows.length]);

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

  const selectedWorkflow = workflows[selectedWorkflowIndex];

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
            <button className="primaryAction fullWidthAction" disabled={isSubmitting || !firstJob || !secondJob || !pool}>
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
            {selectedWorkflow ? (
              <article className="workflowCard workflowFocusCard" key={selectedWorkflow.id}>
                <div className="poolTop">
                  <div>
                    <h2 title={selectedWorkflow.name}>{selectedWorkflow.name}</h2>
                    <p title={`${selectedWorkflow.id} / ${selectedWorkflow.status}`}>{selectedWorkflow.id} / {selectedWorkflow.status}</p>
                  </div>
                  <span>{selectedWorkflow.steps.length} steps</span>
                </div>
                <div className="workflowChain" aria-label={`${selectedWorkflow.name} steps`}>
                  {selectedWorkflow.steps.map((step, index) => (
                    <Fragment key={step.id}>
                      <div className="workflowChainSegment">
                      <div className="workflowChainNode">
                        <div className="workflowNodeIcon">
                          <FileCode2 size={18} aria-hidden="true" />
                        </div>
                        <div>
                          <span>Step {step.position}</span>
                          <strong title={step.name}>{step.name}</strong>
                          <p title={step.job_definition_id}>{step.job_definition_id}</p>
                        </div>
                      </div>
                    </div>
                      {index < selectedWorkflow.steps.length - 1 ? <div className="workflowConnector" aria-hidden="true" /> : null}
                    </Fragment>
                  ))}
                </div>
                <div className="workflowPager">
                  <button className="secondaryButton" disabled={selectedWorkflowIndex <= 0} onClick={() => setSelectedWorkflowIndex((current) => current - 1)}>
                    <ChevronLeft size={15} aria-hidden="true" />
                    Prev
                  </button>
                  <span>{selectedWorkflowIndex + 1} / {workflows.length}</span>
                  <button className="secondaryButton" disabled={selectedWorkflowIndex >= workflows.length - 1} onClick={() => setSelectedWorkflowIndex((current) => current + 1)}>
                    Next
                    <ChevronRight size={15} aria-hidden="true" />
                  </button>
                </div>
              </article>
            ) : null}
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
