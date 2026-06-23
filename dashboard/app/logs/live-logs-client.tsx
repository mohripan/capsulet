"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { CheckCircle2, CircleDot, FileDown, ListFilter, Pause, Play, RefreshCw, Search, TerminalSquare } from "lucide-react";
import { DashboardShell, DateTimePicker, PageHeader, PanelTitle, StateBadge, defaultDateTimeRange } from "../components";
import {
  Workflow,
  WorkflowRun,
  WorkflowRunLogsResponse,
  getErrorMessage,
  getWorkflowRunLogs,
  listWorkflowRuns,
  listWorkflows
} from "../lib/api";

const defaultPollMs = 2000;
const workflowRunStates = ["queued", "running", "succeeded", "failed", "timed_out", "cancelled"];
const defaultRunRange = defaultDateTimeRange();

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "-";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "medium"
  }).format(date);
}

function workflowName(workflows: Workflow[], workflowId: string) {
  return workflows.find((workflow) => workflow.id === workflowId)?.name || workflowId;
}

function isWorkflowTerminal(status: WorkflowRun["status"]) {
  return status === "succeeded" || status === "failed" || status === "cancelled" || status === "timed_out";
}

export default function LiveLogsClient() {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [selectedWorkflowId, setSelectedWorkflowId] = useState("");
  const [selectedRunId, setSelectedRunId] = useState("");
  const [startAt, setStartAt] = useState(defaultRunRange.start);
  const [endAt, setEndAt] = useState(defaultRunRange.end);
  const [stateFilter, setStateFilter] = useState("");
  const [query, setQuery] = useState("");
  const [lineQuery, setLineQuery] = useState("");
  const [logs, setLogs] = useState<WorkflowRunLogsResponse | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [pollIntervalMs, setPollIntervalMs] = useState<number | null>(defaultPollMs);
  const [error, setError] = useState<string | null>(null);
  const terminalRef = useRef<HTMLDivElement | null>(null);

  const refreshRuns = useCallback(async () => {
    setError(null);
    const [workflowResponse, runResponse] = await Promise.all([
      listWorkflows(),
      listWorkflowRuns({
        limit: 200,
        start_at: startAt,
        end_at: endAt,
        q: selectedWorkflowId,
        state: stateFilter,
        sort: "created_at",
        direction: "desc"
      })
    ]);
    setWorkflows(workflowResponse.workflows);
    const filteredRuns = runResponse.workflow_runs.filter((run) => {
      const matchesWorkflow = !selectedWorkflowId || run.workflow_id === selectedWorkflowId;
      const searchable = `${run.id} ${run.workflow_id} ${run.automation_id ?? ""}`.toLowerCase();
      const matchesQuery = !query.trim() || searchable.includes(query.trim().toLowerCase());
      return matchesWorkflow && matchesQuery;
    });
    setWorkflowRuns(filteredRuns);
    setSelectedRunId((current) => (filteredRuns.some((run) => run.id === current) ? current : filteredRuns[0]?.id || ""));
  }, [endAt, query, selectedWorkflowId, startAt, stateFilter]);

  const refreshLogs = useCallback(async () => {
    if (!selectedRunId) {
      setLogs(null);
      return;
    }
    const response = await getWorkflowRunLogs(selectedRunId);
    setLogs(response);
  }, [selectedRunId]);

  const refresh = useCallback(async (options: { showLoading?: boolean; includeRuns?: boolean } = {}) => {
    const showLoading = options.showLoading ?? true;
    const includeRuns = options.includeRuns ?? true;
    if (showLoading) setIsLoading(true);
    setError(null);
    try {
      if (includeRuns) await refreshRuns();
      await refreshLogs();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      if (showLoading) setIsLoading(false);
    }
  }, [refreshLogs, refreshRuns]);

  useEffect(() => {
    void refresh({ showLoading: true, includeRuns: true });
  }, [refresh]);

  useEffect(() => {
    if (pollIntervalMs === null) return;
    const timer = window.setInterval(() => {
      void refresh({ showLoading: false, includeRuns: false });
    }, pollIntervalMs);
    return () => window.clearInterval(timer);
  }, [pollIntervalMs, refresh]);

  useEffect(() => {
    terminalRef.current?.scrollTo({ top: terminalRef.current.scrollHeight });
  }, [logs]);

  const selectedRun = useMemo(
    () => workflowRuns.find((run) => run.id === selectedRunId) ?? null,
    [selectedRunId, workflowRuns]
  );
  const visibleEntries = useMemo(() => {
    const needle = lineQuery.trim().toLowerCase();
    if (!needle) return logs?.entries ?? [];
    return (logs?.entries ?? []).filter((entry) =>
      `${entry.job_run_id ?? ""} ${entry.workflow_step_id} ${entry.status} ${entry.logs}`.toLowerCase().includes(needle)
    );
  }, [lineQuery, logs]);
  const capturedLineCount = useMemo(
    () => (logs?.entries ?? []).reduce((count, entry) => count + (entry.logs ? entry.logs.split(/\r?\n/).filter(Boolean).length : 0), 0),
    [logs]
  );
  const shouldPoll = pollIntervalMs !== null && (!selectedRun || !isWorkflowTerminal(selectedRun.status));
  const currentJobRunId = selectedRun?.step_runs.find((step) => step.position === selectedRun.current_step_position)?.job_run_id;

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Live workflow logs"
        title="Watch running workflow containers"
        description="Choose a workflow run, filter step output, and keep the terminal view polling while jobs are still moving."
      />

      <section className="contentGrid liveLogsLayout">
        <section className="panel span4 liveLogsSidebar">
          <PanelTitle icon={ListFilter} title="Workflow Runs" action={`${workflowRuns.length} visible`} />
          <div className="formStack logSelectors">
            <label>
              <span>Workflow</span>
              <select value={selectedWorkflowId} onChange={(event) => setSelectedWorkflowId(event.target.value)}>
                <option value="">All workflows</option>
                {workflows.map((workflow) => (
                  <option value={workflow.id} key={workflow.id}>
                    {workflow.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Start</span>
              <DateTimePicker value={startAt} onChange={setStartAt} />
            </label>
            <label>
              <span>End</span>
              <DateTimePicker value={endAt} onChange={setEndAt} />
            </label>
            <label>
              <span>State</span>
              <select value={stateFilter} onChange={(event) => setStateFilter(event.target.value)}>
                <option value="">All states</option>
                {workflowRunStates.map((state) => (
                  <option value={state} key={state}>
                    {state}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Run search</span>
              <div className="logSearchInput">
                <Search size={15} aria-hidden="true" />
                <input value={query} onChange={(event) => setQuery(event.target.value)} placeholder="Run, workflow, automation" />
              </div>
            </label>
          </div>

          <div className="logRunList">
            {workflowRuns.map((run) => (
              <button
                className={run.id === selectedRunId ? "logRunItem active" : "logRunItem"}
                key={run.id}
                type="button"
                onClick={() => setSelectedRunId(run.id)}
              >
                <span className={isWorkflowTerminal(run.status) ? "logRunDot finished" : "logRunDot"} />
                <strong>{workflowName(workflows, run.workflow_id)}</strong>
                <span className="mono">{run.id}</span>
                <StateBadge state={run.status} />
              </button>
            ))}
            {!isLoading && workflowRuns.length === 0 ? <div className="emptyState">No workflow runs match these filters.</div> : null}
          </div>
        </section>

        <section className="panel span8 liveLogsMain">
          <div className="logConsoleHeader">
            <PanelTitle icon={TerminalSquare} title="Live Output" action={shouldPoll ? `${pollIntervalMs! / 1000}s polling` : "Manual"} />
            <div className="logConsoleActions">
              <select
                aria-label="Log refresh mode"
                className="logRefreshSelect"
                value={pollIntervalMs === null ? "manual" : String(pollIntervalMs)}
                onChange={(event) => setPollIntervalMs(event.target.value === "manual" ? null : Number(event.target.value))}
              >
                <option value="manual">Manual refresh</option>
                <option value="2000">Poll every 2s</option>
                <option value="5000">Poll every 5s</option>
                <option value="10000">Poll every 10s</option>
              </select>
              <button className="secondaryButton" type="button" onClick={() => setPollIntervalMs((current) => (current === null ? defaultPollMs : null))}>
                {pollIntervalMs !== null ? <Pause size={16} aria-hidden="true" /> : <Play size={16} aria-hidden="true" />}
                {pollIntervalMs !== null ? "Pause" : "Resume"}
              </button>
              <button className="secondaryButton" type="button" onClick={() => refresh({ showLoading: true, includeRuns: true })} disabled={isLoading}>
                <RefreshCw size={16} aria-hidden="true" />
                {isLoading ? "Refreshing" : "Refresh"}
              </button>
            </div>
          </div>

          {error ? <div className="errorBox">{error}</div> : null}
          {selectedRun ? (
            <div className="logRunSummary">
              <div>
                <span>Workflow</span>
                <strong>{workflowName(workflows, selectedRun.workflow_id)}</strong>
              </div>
              <div>
                <span>Run</span>
                <strong className="mono">{selectedRun.id}</strong>
              </div>
              <div>
                <span>Created</span>
                <strong>{formatDateTime(selectedRun.created_at)}</strong>
              </div>
              <div>
                <span>Captured lines</span>
                <strong>{capturedLineCount}</strong>
              </div>
            </div>
          ) : null}

          <div className="logToolbar">
            <label>
              <Search size={15} aria-hidden="true" />
              <input value={lineQuery} onChange={(event) => setLineQuery(event.target.value)} placeholder="Filter logs, job run, step, or state" />
            </label>
            {logs?.entries.some((entry) => entry.object_log_available) ? (
              <span className="objectLogHint">
                <FileDown size={15} aria-hidden="true" />
                Full stdout artifact available
              </span>
            ) : null}
          </div>

          <div className="liveTerminal" ref={terminalRef}>
            {!selectedRunId ? <div className="emptyState">Select a workflow run to start watching logs.</div> : null}
            {selectedRunId && !logs && isLoading ? <div className="emptyState">Loading workflow logs.</div> : null}
            {logs && visibleEntries.length === 0 ? <div className="emptyState">No log entries match this filter.</div> : null}
            {visibleEntries.map((entry) => (
              <article className="logStepBlock" key={entry.step_run_id}>
                <header>
                  {entry.status === "succeeded" ? <CheckCircle2 size={16} aria-hidden="true" /> : <CircleDot size={16} aria-hidden="true" />}
                  <strong>Step {entry.position}</strong>
                  <span className="mono">{entry.job_run_id ?? "no job run"}</span>
                  <StateBadge state={entry.status} />
                </header>
                <pre>{entry.logs || "No logs captured for this step yet."}</pre>
              </article>
            ))}
          </div>

          {selectedRun ? (
            <div className="logFooter">
              {currentJobRunId ? <Link href={`/runs/${currentJobRunId}`}>Open current job run</Link> : <span>No active job run yet</span>}
              <span>{pollIntervalMs === null ? "Manual refresh" : `${pollIntervalMs / 1000}s log refresh`}</span>
            </div>
          ) : null}
        </section>
      </section>
    </DashboardShell>
  );
}
