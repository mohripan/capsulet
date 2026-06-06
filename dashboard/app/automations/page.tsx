"use client";

import { FormEvent, useEffect, useState } from "react";
import { Clock3, Play, Plus, RefreshCw, Send, Workflow } from "lucide-react";
import Link from "next/link";
import { DashboardShell, PageHeader, PanelTitle, StateBadge } from "../components";
import {
  Automation,
  Workflow as WorkflowDefinition,
  WorkflowRun,
  createAutomation,
  getErrorMessage,
  listAutomations,
  listWorkflowRuns,
  listWorkflows,
  triggerAutomation
} from "../lib/api";

export default function AutomationsPage() {
  const [automations, setAutomations] = useState<Automation[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowDefinition[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [name, setName] = useState("Hourly email automation");
  const [workflowId, setWorkflowId] = useState("");
  const [triggerKind, setTriggerKind] = useState<"manual" | "interval">("manual");
  const [intervalSeconds, setIntervalSeconds] = useState(3600);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const [automationResponse, workflowResponse, runResponse] = await Promise.all([
        listAutomations(),
        listWorkflows(),
        listWorkflowRuns()
      ]);
      setAutomations(automationResponse.automations);
      setWorkflows(workflowResponse.workflows);
      setWorkflowRuns(runResponse.workflow_runs);
      setWorkflowId((current) => current || workflowResponse.workflows[0]?.id || "");
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
    setMessage(null);
    try {
      const automation = await createAutomation({
        name,
        workflow_id: workflowId,
        trigger_kind: triggerKind,
        interval_seconds: triggerKind === "interval" ? intervalSeconds : undefined
      });
      setMessage(`Created ${automation.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  async function trigger(id: string) {
    setError(null);
    setMessage(null);
    try {
      const run = await triggerAutomation(id);
      setMessage(`Triggered workflow run ${run.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Automation authoring"
        title="Create manual or interval automations"
        description="Bind a workflow to a manual trigger or an interval trigger. The scheduler creates workflow runs for interval automations."
      />

      <section className="contentGrid">
        <section className="panel span5">
          <PanelTitle icon={Plus} title="New Automation" action="Live API" />
          <form className="formStack" onSubmit={submit}>
            <label>
              <span>Name</span>
              <input value={name} onChange={(event) => setName(event.target.value)} />
            </label>
            <label>
              <span>Workflow</span>
              <select value={workflowId} onChange={(event) => setWorkflowId(event.target.value)}>
                {workflows.map((workflow) => (
                  <option value={workflow.id} key={workflow.id}>
                    {workflow.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Trigger</span>
              <select value={triggerKind} onChange={(event) => setTriggerKind(event.target.value as "manual" | "interval")}>
                <option value="manual">manual</option>
                <option value="interval">interval</option>
              </select>
            </label>
            {triggerKind === "interval" ? (
              <label>
                <span>Interval seconds</span>
                <input
                  type="number"
                  min="30"
                  value={intervalSeconds}
                  onChange={(event) => setIntervalSeconds(Number(event.target.value))}
                />
              </label>
            ) : null}
            <button className="primaryAction inlineAction" disabled={isSubmitting || !workflowId}>
              <Send size={16} aria-hidden="true" />
              {isSubmitting ? "Creating" : "Create automation"}
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={Workflow} title="Automations" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {message ? <div className="successBox">{message}</div> : null}
          <div className="resourceList">
            {!isLoading && automations.length === 0 ? (
              <div className="emptyState">No automations yet. Create a workflow, then bind it here.</div>
            ) : null}
            {automations.map((automation) => (
              <article className="resourceRow" key={automation.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    {automation.trigger_kind === "interval" ? <Clock3 size={19} /> : <Play size={19} />}
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p>{automation.workflow_id}</p>
                  </div>
                </div>
                <span className="tableCell">{automation.trigger_kind}</span>
                <span className="tableCell">{automation.interval_seconds ? `${automation.interval_seconds}s` : "-"}</span>
                <button className="secondaryButton" onClick={() => void trigger(automation.id)}>
                  <Play size={15} aria-hidden="true" />
                  Trigger
                </button>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Clock3} title="Workflow Runs" action="Scheduler" />
          <div className="runTable">
            <div className="runHeader liveRunGrid">
              <span>Workflow run</span>
              <span>Workflow</span>
              <span>Job runs</span>
              <span>State</span>
              <span>Automation</span>
            </div>
            {workflowRuns.map((run) => (
              <div className="runRow liveRunGrid" key={run.id}>
                <span className="mono tableCell" title={run.id}>{run.id}</span>
                <span className="tableCell" title={run.workflow_id}>{run.workflow_id}</span>
                <span className="stepRunLinks">
                  {run.step_runs.length === 0 ? (
                    "-"
                  ) : (
                    run.step_runs.map((stepRun) => (
                      <Link href={`/runs/${stepRun.job_run_id}`} key={stepRun.id} title={stepRun.job_run_id}>
                        {stepRun.position}: {stepRun.status}
                      </Link>
                    ))
                  )}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell" title={run.automation_id ?? ""}>{run.automation_id ?? "-"}</span>
              </div>
            ))}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
