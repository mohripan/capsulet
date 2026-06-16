"use client";

import Link from "next/link";
import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { Activity, FileCode2, ListFilter, Play, RefreshCw, Send } from "lucide-react";
import { DashboardShell, DateTimePicker, PageHeader, PanelTitle, PythonEditor, ResizableGridTable, StateBadge, defaultDateTimeRange } from "../components";
import {
  ExecutionPool,
  JobDefinition,
  JobRun,
  getErrorMessage,
  listExecutionPools,
  listJobDefinitions,
  listRuns,
  submitRun
} from "../lib/api";

const runColumns = [
  { label: "Run", width: 250, minWidth: 150, sortKey: "run" },
  { label: "Created", width: 190, minWidth: 160, sortKey: "created_at" },
  { label: "Job definition", width: 250, minWidth: 160, sortKey: "job_definition" },
  { label: "Pool", width: 170, minWidth: 110, sortKey: "pool" },
  { label: "State", width: 150, minWidth: 120, sortKey: "state" },
  { label: "Attempts", width: 130, minWidth: 92, sortKey: "attempts" }
];

const pageSize = 8;
const runStates = ["queued", "leased", "running", "retry_scheduled", "succeeded", "failed", "timed_out", "cancelled"];
const defaultRunRange = defaultDateTimeRange();

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "-";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(date);
}

export default function RunsClient() {
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [runPage, setRunPage] = useState(1);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const [createdRun, setCreatedRun] = useState<JobRun | null>(null);
  const [filterStartAt, setFilterStartAt] = useState(defaultRunRange.start);
  const [filterEndAt, setFilterEndAt] = useState(defaultRunRange.end);
  const [filterText, setFilterText] = useState("");
  const [filterState, setFilterState] = useState("");
  const [sortKey, setSortKey] = useState("created_at");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("desc");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [jobDefinitions, setJobDefinitions] = useState<JobDefinition[]>([]);
  const [executionPools, setExecutionPools] = useState<ExecutionPool[]>([]);
  const [jobDefinitionId, setJobDefinitionId] = useState<string>("");
  const [jobPool, setJobPool] = useState("");
  const [scriptPool, setScriptPool] = useState("");
  const [script, setScript] = useState("print('hello from dashboard')");

  const refresh = useCallback(async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listRuns({
        limit: 200,
        start_at: filterStartAt,
        end_at: filterEndAt,
        q: filterText,
        state: filterState,
        sort: sortKey,
        direction: sortDirection
      });
      setRuns(response.runs);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, [filterEndAt, filterStartAt, filterState, filterText, sortDirection, sortKey]);

  useEffect(() => {
    void refreshAuthoringData();
  }, []);

  useEffect(() => {
    setRunPage(1);
    void refresh();
  }, [refresh]);

  async function refreshAuthoringData() {
    try {
      const [definitionsResponse, poolsResponse] = await Promise.all([listJobDefinitions(), listExecutionPools()]);
      setJobDefinitions(definitionsResponse.job_definitions);
      setExecutionPools(poolsResponse.execution_pools);
      setJobDefinitionId((current) => current || definitionsResponse.job_definitions[0]?.id || "");
      const defaultPool = poolsResponse.execution_pools.find((pool) => pool.is_default)?.name || poolsResponse.execution_pools[0]?.name || "";
      setJobPool((current) => current || defaultPool);
      setScriptPool((current) => current || defaultPool);
    } catch (err) {
      setSubmissionError(getErrorMessage(err));
    }
  }

  const counts = useMemo(() => {
    return runs.reduce<Record<string, number>>((acc, run) => {
      acc[run.status] = (acc[run.status] ?? 0) + 1;
      return acc;
    }, {});
  }, [runs]);

  const pagedRuns = runs.slice((runPage - 1) * pageSize, runPage * pageSize);

  function handleSort(nextSortKey: string) {
    if (nextSortKey === sortKey) {
      setSortDirection((current) => (current === "asc" ? "desc" : "asc"));
      return;
    }
    setSortKey(nextSortKey);
    setSortDirection("asc");
  }

  useEffect(() => {
    const pages = Math.max(1, Math.ceil(runs.length / pageSize));
    if (runPage > pages) {
      setRunPage(pages);
    }
  }, [runPage, runs.length]);

  async function submitDefinitionJob(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!jobDefinitionId) {
      setSubmissionError("Create a job definition before submitting a reusable job.");
      return;
    }
    await submitDashboardRun({
      job_definition_id: jobDefinitionId,
      execution_pool: jobPool
    });
  }

  async function submitScript(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    if (!script.trim()) {
      setSubmissionError("Python script cannot be empty.");
      return;
    }
    await submitDashboardRun({
      job_definition_id: "script",
      execution_pool: scriptPool,
      python_script: script
    });
  }

  async function submitDashboardRun(request: {
    job_definition_id: string;
    execution_pool: string;
    python_script?: string;
  }) {
    if (!request.execution_pool.trim()) {
      setSubmissionError("Execution pool cannot be empty.");
      return;
    }

    setIsSubmitting(true);
    setSubmissionError(null);
    setCreatedRun(null);
    try {
      const run = await submitRun(request);
      setCreatedRun(run);
      await refresh();
    } catch (err) {
      setSubmissionError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Run queue"
        title="Submit and inspect live job runs"
        description="Create seeded jobs or single-file Python scripts, then track state, attempts, logs, and artifacts from the API."
      />

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Activity} title="Runs" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          <div className="tableFilters">
            <label>
              <span>Start</span>
              <DateTimePicker value={filterStartAt} onChange={setFilterStartAt} />
            </label>
            <label>
              <span>End</span>
              <DateTimePicker value={filterEndAt} onChange={setFilterEndAt} />
            </label>
            <label>
              <span>Name</span>
              <input value={filterText} onChange={(event) => setFilterText(event.target.value)} placeholder="Run or job definition" />
            </label>
            <label>
              <span>State</span>
              <select value={filterState} onChange={(event) => setFilterState(event.target.value)}>
                <option value="">All states</option>
                {runStates.map((state) => <option value={state} key={state}>{state}</option>)}
              </select>
            </label>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          <ResizableGridTable columns={runColumns} sortKey={sortKey} sortDirection={sortDirection} onSort={handleSort}>
            {!isLoading && runs.length === 0 ? (
              <div className="emptyState">No runs yet. Submit a seeded job or Python script to create one.</div>
            ) : null}
            {pagedRuns.map((run) => (
              <Link className="resizableRow runRow interactiveRow" href={`/runs/${run.id}`} key={run.id}>
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
              </Link>
            ))}
          </ResizableGridTable>
          <Pagination page={runPage} total={runs.length} onPage={setRunPage} />
        </section>

        <section className="panel span4">
          <PanelTitle icon={ListFilter} title="Run States" action="Summary" />
          <div className="tileGrid">
            {["queued", "leased", "running", "retry_scheduled", "succeeded", "failed", "timed_out", "cancelled"].map(
              (state) => (
                <div className="miniTile" key={state}>
                  <strong>{counts[state] ?? 0}</strong>
                  <span>{state}</span>
                </div>
              )
            )}
          </div>
        </section>

        <section className="panel span5">
          <PanelTitle icon={Play} title="Reusable Job" action="Submit" />
          <form className="formStack" onSubmit={submitDefinitionJob}>
            <label>
              <span>Job definition</span>
              <select value={jobDefinitionId} onChange={(event) => setJobDefinitionId(event.target.value)}>
                {jobDefinitions.map((definition) => (
                  <option value={definition.id} key={definition.id}>
                    {definition.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Execution pool</span>
              <select value={jobPool} onChange={(event) => setJobPool(event.target.value)}>
                {executionPools.map((pool) => (
                  <option value={pool.name} key={pool.name}>
                    {pool.name}
                  </option>
                ))}
              </select>
            </label>
            <button className="primaryAction inlineAction" disabled={isSubmitting}>
              <Send size={16} aria-hidden="true" />
              Submit job definition
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={FileCode2} title="Python Script" action="Submit" />
          <form className="formStack" onSubmit={submitScript}>
            <label>
              <span>Execution pool</span>
              <select value={scriptPool} onChange={(event) => setScriptPool(event.target.value)}>
                {executionPools.map((pool) => (
                  <option value={pool.name} key={pool.name}>
                    {pool.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Script</span>
              <PythonEditor value={script} onChange={setScript} rows={8} />
            </label>
            <button className="primaryAction inlineAction" disabled={isSubmitting}>
              <Send size={16} aria-hidden="true" />
              Submit script
            </button>
          </form>
        </section>

        {submissionError || createdRun ? (
          <section className="panel span12">
            {submissionError ? <div className="errorBox">{submissionError}</div> : null}
            {createdRun ? (
              <div className="successBox">
                Created <Link href={`/runs/${createdRun.id}`}>{createdRun.id}</Link> in pool {createdRun.execution_pool}.
              </div>
            ) : null}
          </section>
        ) : null}
      </section>
    </DashboardShell>
  );
}

function Pagination({ page, total, onPage }: { page: number; total: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <div className="pagination">
      <button className="secondaryButton" disabled={page <= 1} onClick={() => onPage(page - 1)}>
        Prev
      </button>
      <span>{page} / {pages}</span>
      <button className="secondaryButton" disabled={page >= pages} onClick={() => onPage(page + 1)}>
        Next
      </button>
    </div>
  );
}
