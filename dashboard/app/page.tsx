"use client";

import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  AlertTriangle,
  Braces,
  CheckCircle2,
  CircleDot,
  Clock3,
  FileCode2,
  Gauge,
  GitBranch,
  Network,
  Pause,
  Play,
  Route,
  TerminalSquare,
  Workflow,
  Zap
} from "lucide-react";
import { DashboardShell, DateTimePicker, PanelTitle, ResizableGridTable, StateBadge, defaultDateTimeRange } from "./components";
import {
  Automation,
  ExecutionPool,
  JobRun,
  Workflow as ApiWorkflow,
  WorkflowRun,
  WorkflowRunLogsResponse,
  getWorkflowRunLogs,
  getErrorMessage,
  listAutomations,
  listExecutionPools,
  listRuns,
  listWorkflowRuns,
  listWorkflows
} from "./lib/api";
import type { LucideIcon } from "lucide-react";

const recentRunColumns = [
  { label: "Run", width: 220, minWidth: 140 },
  { label: "Created", width: 190, minWidth: 160 },
  { label: "Job definition", width: 230, minWidth: 150 },
  { label: "Pool", width: 110, minWidth: 80 },
  { label: "State", width: 160, minWidth: 120 },
  { label: "Attempts", width: 110, minWidth: 84 },
  { label: "Host group", width: 170, minWidth: 120 }
];

const workflowRunColumns = [
  { label: "Workflow run", width: 250, minWidth: 160 },
  { label: "Created", width: 190, minWidth: 160 },
  { label: "Workflow", width: 250, minWidth: 160 },
  { label: "Job runs", width: 230, minWidth: 150 },
  { label: "State", width: 150, minWidth: 120 },
  { label: "Automation", width: 230, minWidth: 150 }
];
const defaultOverviewRange = defaultDateTimeRange();

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "-";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(date);
}

export default function OverviewPage() {
  const [automations, setAutomations] = useState<Automation[]>([]);
  const [pools, setPools] = useState<ExecutionPool[]>([]);
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [workflows, setWorkflows] = useState<ApiWorkflow[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [overviewStartAt, setOverviewStartAt] = useState(defaultOverviewRange.start);
  const [overviewEndAt, setOverviewEndAt] = useState(defaultOverviewRange.end);
  const [selectedLogRunId, setSelectedLogRunId] = useState("");
  const [overviewLogs, setOverviewLogs] = useState<WorkflowRunLogsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [errors, setErrors] = useState<string[]>([]);

  const refresh = useCallback(async function refresh() {
    setIsLoading(true);
    setErrors([]);
    const [automationResult, poolResult, runResult, workflowResult, workflowRunResult] = await Promise.allSettled([
      listAutomations(),
      listExecutionPools(),
      listRuns({ limit: 50, start_at: overviewStartAt, end_at: overviewEndAt }),
      listWorkflows(),
      listWorkflowRuns({ limit: 100, start_at: overviewStartAt, end_at: overviewEndAt })
    ]);

    const nextErrors: string[] = [];
    if (automationResult.status === "fulfilled") {
      setAutomations(automationResult.value.automations);
    } else {
      setAutomations([]);
      nextErrors.push(`Automations: ${getErrorMessage(automationResult.reason)}`);
    }
    if (poolResult.status === "fulfilled") {
      setPools(poolResult.value.execution_pools);
    } else {
      setPools([]);
      nextErrors.push(`Execution pools: ${getErrorMessage(poolResult.reason)}`);
    }
    if (runResult.status === "fulfilled") {
      setRuns(runResult.value.runs);
    } else {
      setRuns([]);
      nextErrors.push(`Runs: ${getErrorMessage(runResult.reason)}`);
    }
    if (workflowResult.status === "fulfilled") {
      setWorkflows(workflowResult.value.workflows);
    } else {
      setWorkflows([]);
      nextErrors.push(`Workflows: ${getErrorMessage(workflowResult.reason)}`);
    }
    if (workflowRunResult.status === "fulfilled") {
      setWorkflowRuns(workflowRunResult.value.workflow_runs);
    } else {
      setWorkflowRuns([]);
      nextErrors.push(`Workflow runs: ${getErrorMessage(workflowRunResult.reason)}`);
    }

    setErrors(nextErrors);
    setIsLoading(false);
  }, [overviewEndAt, overviewStartAt]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const metrics = useMemo(() => {
    const running = runs.filter((run) => run.status === "running" || run.status === "leased").length;
    const queued = runs.filter((run) => run.status === "queued" || run.status === "retry_scheduled").length;
    const failed = runs.filter((run) => ["failed", "timed_out", "cancelled"].includes(run.status)).length;
    const terminal = runs.filter((run) => ["succeeded", "failed", "timed_out", "cancelled"].includes(run.status));
    const succeeded = terminal.filter((run) => run.status === "succeeded").length;
    return {
      running: runs.length ? String(running) : "No data",
      queued: runs.length ? String(queued) : "No data",
      success: terminal.length ? `${((succeeded / terminal.length) * 100).toFixed(1)}%` : "No data",
      failed: runs.length ? String(failed) : "No data"
    };
  }, [runs]);

  const workflowById = useMemo(() => new Map(workflows.map((workflow) => [workflow.id, workflow])), [workflows]);
  const selectedAutomation = automations[0];
  const selectedWorkflow = selectedAutomation ? workflowById.get(selectedAutomation.workflow_id) : undefined;
  const overviewLogRuns = useMemo(() => workflowRuns.slice(0, 5), [workflowRuns]);
  const selectedLogRun = overviewLogRuns.find((run) => run.id === selectedLogRunId) ?? overviewLogRuns[0];
  const overviewLogText = useMemo(() => {
    if (!overviewLogs?.entries.length) return "";
    return overviewLogs.entries
      .map((entry) => [`Step ${entry.position} / ${entry.status}`, entry.logs || "No logs captured for this step yet."].join("\n"))
      .join("\n\n");
  }, [overviewLogs]);

  useEffect(() => {
    setSelectedLogRunId((current) => (overviewLogRuns.some((run) => run.id === current) ? current : overviewLogRuns[0]?.id || ""));
  }, [overviewLogRuns]);

  useEffect(() => {
    if (!selectedLogRun?.id) {
      setOverviewLogs(null);
      return;
    }
    let isActive = true;
    getWorkflowRunLogs(selectedLogRun.id)
      .then((response) => {
        if (isActive) setOverviewLogs(response);
      })
      .catch((err) => {
        if (!isActive) return;
        setOverviewLogs(null);
        setErrors((current) => [...current, `Live logs: ${getErrorMessage(err)}`]);
      });
    return () => {
      isActive = false;
    };
  }, [selectedLogRun?.id]);

  return (
    <DashboardShell>
      <section className="heroBand">
        <div className="heroText">
          <div className="eyebrow">
            <CircleDot size={14} aria-hidden="true" />
            Live cluster overview
          </div>
          <h1>Automation runs across Kubernetes execution pools</h1>
          <p>Route jobs from manual, scheduled, webhook, and dependency triggers into the right compute pool.</p>
        </div>
        <div className="heroStats" aria-label="Run summary">
          <Metric icon={Zap} label="Running" value={isLoading ? "Loading" : metrics.running} tone="good" />
          <Metric icon={Clock3} label="Queued" value={isLoading ? "Loading" : metrics.queued} tone="warn" />
          <Metric icon={CheckCircle2} label="Success" value={isLoading ? "Loading" : metrics.success} tone="good" />
          <Metric icon={AlertTriangle} label="Failed" value={isLoading ? "Loading" : metrics.failed} tone="bad" />
        </div>
      </section>

      {errors.length ? <div className="errorBox">{errors.join(" | ")}</div> : null}

      <section className="contentGrid">
        <section className="panel span12">
          <PanelTitle icon={Clock3} title="Overview Range" action="Date picker" />
          <div className="tableFilters overviewRangeFilters">
            <label>
              <span>Start</span>
              <DateTimePicker value={overviewStartAt} onChange={setOverviewStartAt} />
            </label>
            <label>
              <span>End</span>
              <DateTimePicker value={overviewEndAt} onChange={setOverviewEndAt} />
            </label>
          </div>
        </section>

        <section className="panel span8">
          <PanelTitle icon={Workflow} title="Automations" action="Live API" />
          <div className="automationList">
            {!isLoading && automations.length === 0 ? <div className="emptyState">No automations exist in the backend.</div> : null}
            {automations.slice(0, 3).map((automation) => (
              <article className="automationRow" key={automation.id}>
                <div className="automationMain">
                  <div className="automationIcon">
                    <Workflow size={19} aria-hidden="true" />
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p title={automation.workflow_id}>{automation.workflow_id}</p>
                  </div>
                </div>
                <div className="triggerExpr">
                  <Braces size={16} aria-hidden="true" />
                  <span title={conditionToText(automation.condition)}>{conditionToText(automation.condition)}</span>
                </div>
                <div className="poolPill">
                  <Route size={15} aria-hidden="true" />
                  {workflowById.get(automation.workflow_id)?.steps[0]?.execution_pool || "No pool"}
                </div>
                <div className={automation.status === "enabled" ? "status enabled" : "status paused"} title={automation.status}>
                  {automation.status === "enabled" ? <Play size={14} aria-hidden="true" /> : <Pause size={14} aria-hidden="true" />}
                  {automation.status}
                </div>
                <div className="successCell">
                  <strong>{automation.triggers.length}</strong>
                  <span>{automation.trigger_kind}</span>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={GitBranch} title="Condition" action="Live API" />
          {selectedAutomation ? (
            <>
              <div className="logicBuilder">
                <code className="conditionPreview">{conditionToText(selectedAutomation.condition)}</code>
              </div>
              <div className="targetBox">
                <div>
                  <span>Target workflow</span>
                  <strong title={selectedAutomation.workflow_id}>{selectedWorkflow?.name || selectedAutomation.workflow_id}</strong>
                </div>
                <div>
                  <span>Execution pool</span>
                  <strong>{selectedWorkflow?.steps[0]?.execution_pool || "No pool"}</strong>
                </div>
              </div>
            </>
          ) : (
            <div className="emptyState">No automation condition exists in the backend.</div>
          )}
        </section>

        <section className="panel span7">
          <PanelTitle icon={Activity} title="Recent Runs" action="Live API" />
          <ResizableGridTable columns={recentRunColumns}>
            {!isLoading && runs.length === 0 ? <div className="emptyState">No job runs exist in the backend.</div> : null}
            {runs.slice(0, 4).map((run) => (
              <div className="resizableRow runRow" key={run.id}>
                <span className="mono tableCell" title={run.id}>
                  {run.id}
                </span>
                <span className="tableCell" title={run.created_at}>
                  {formatDateTime(run.created_at)}
                </span>
                <span className="tableCell" title={run.job_definition_id}>
                  {run.job_definition_id}
                </span>
                <span className="tableCell" title={run.execution_pool}>
                  {run.execution_pool}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell" title={String(run.attempt_count)}>
                  {run.attempt_count}
                </span>
                <span className="tableCell" title={run.host_group}>
                  {run.host_group || "No host group"}
                </span>
              </div>
            ))}
          </ResizableGridTable>
        </section>

        <section className="panel span5">
          <PanelTitle icon={Route} title="Execution Pools" action="Live API" />
          <div className="poolStack">
            {!isLoading && pools.length === 0 ? <div className="emptyState">No execution pools exist in the backend.</div> : null}
            {pools.map((pool) => (
              <article className="poolCard flat" key={pool.name}>
                <div className="poolTop">
                  <div>
                    <h2>{pool.name}</h2>
                    <p>{pool.description || "No description"}</p>
                  </div>
                  <span>{pool.is_default ? "default" : "available"}</span>
                </div>
                <div className="poolFoot">
                  <span>{runs.filter((run) => run.execution_pool === pool.name && run.status === "running").length} running</span>
                  <span>{runs.filter((run) => run.execution_pool === pool.name && run.status === "queued").length} queued</span>
                </div>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={Network} title="Topology" action="No endpoint" />
          <div className="emptyState">No topology endpoint exists for the overview.</div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={TerminalSquare} title="Live Logs" action={overviewLogRuns.length ? "Latest runs" : "No runs"} />
          <label className="overviewLogSelect">
            <span>Workflow run</span>
            <select value={selectedLogRun?.id || ""} onChange={(event) => setSelectedLogRunId(event.target.value)}>
              {overviewLogRuns.map((run) => (
                <option value={run.id} key={run.id}>
                  {(workflowById.get(run.workflow_id)?.name || run.workflow_id).slice(0, 42)}
                </option>
              ))}
            </select>
          </label>
          <div className="terminal overviewLogTerminal">
            {!selectedLogRun ? <div className="emptyState">No workflow runs exist in this range.</div> : null}
            {selectedLogRun && !overviewLogText ? <div className="emptyState">No logs captured for this workflow run yet.</div> : null}
            {overviewLogText ? <code>{overviewLogText}</code> : null}
          </div>
        </section>

        <section className="panel span4">
          <PanelTitle icon={FileCode2} title="YAML Export" action="No endpoint" />
          <div className="yamlPreview">No automation export endpoint exists for the overview.</div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Gauge} title="Workflow Runs" action="Live API" />
          <ResizableGridTable columns={workflowRunColumns}>
            {!isLoading && workflowRuns.length === 0 ? <div className="emptyState">No workflow runs exist in the backend.</div> : null}
            {workflowRuns.slice(0, 4).map((run) => (
              <div className="resizableRow runRow" key={run.id}>
                <span className="mono tableCell" title={run.id}>{run.id}</span>
                <span className="tableCell" title={run.created_at}>{formatDateTime(run.created_at)}</span>
                <span className="tableCell" title={run.workflow_id}>{run.workflow_id}</span>
                <span className="stepRunLinks">
                  {run.step_runs.length === 0 ? "No job runs" : run.step_runs.map((stepRun) => `${stepRun.position}: ${stepRun.status}`).join(", ")}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell" title={run.automation_id ?? ""}>{run.automation_id ?? "No automation"}</span>
              </div>
            ))}
          </ResizableGridTable>
        </section>
      </section>
    </DashboardShell>
  );
}

function Metric({
  icon: Icon,
  label,
  value,
  tone
}: {
  icon: LucideIcon;
  label: string;
  value: string;
  tone: string;
}) {
  return (
    <div className={`metric ${tone}`}>
      <Icon size={20} aria-hidden="true" />
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function conditionToText(condition: Automation["condition"]): string {
  if ("trigger" in condition) {
    return condition.trigger;
  }
  if ("all" in condition) {
    return condition.all.map(conditionToText).join(" AND ");
  }
  return condition.any.map(conditionToText).join(" OR ");
}
