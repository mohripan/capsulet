"use client";

import Link from "next/link";
import { FormEvent, useEffect, useMemo, useState } from "react";
import { Activity, FileCode2, ListFilter, Play, RefreshCw, Send } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, StateBadge } from "../components";
import { JobRun, getErrorMessage, listRuns, submitRun } from "../lib/api";

const seededJobs = [
  ["job_hello_python", "Hello Python"],
  ["job_fail_python", "Failure and retry"],
  ["job_timeout_python", "Timeout"],
  ["job_artifact_python", "Artifact producer"]
] as const;

const pools = ["mini", "large"];

export default function RunsClient() {
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [submissionError, setSubmissionError] = useState<string | null>(null);
  const [createdRun, setCreatedRun] = useState<JobRun | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [jobDefinitionId, setJobDefinitionId] = useState<string>(seededJobs[0][0]);
  const [jobPool, setJobPool] = useState("mini");
  const [scriptPool, setScriptPool] = useState("mini");
  const [script, setScript] = useState("print('hello from dashboard')");

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listRuns();
      setRuns(response.runs);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  const counts = useMemo(() => {
    return runs.reduce<Record<string, number>>((acc, run) => {
      acc[run.status] = (acc[run.status] ?? 0) + 1;
      return acc;
    }, {});
  }, [runs]);

  async function submitSeededJob(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
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
    <DashboardShell actionLabel="Submit run">
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
          {error ? <div className="errorBox">{error}</div> : null}
          <div className="runTable">
            <div className="runHeader liveRunGrid">
              <span>Run</span>
              <span>Job definition</span>
              <span>Pool</span>
              <span>State</span>
              <span>Attempts</span>
            </div>
            {!isLoading && runs.length === 0 ? (
              <div className="emptyState">No runs yet. Submit a seeded job or Python script to create one.</div>
            ) : null}
            {runs.map((run) => (
              <Link className="runRow liveRunGrid interactiveRow" href={`/runs/${run.id}`} key={run.id}>
                <span className="mono">{run.id}</span>
                <span>{run.job_definition_id}</span>
                <span>{run.execution_pool}</span>
                <StateBadge state={run.status} />
                <span>{run.attempt_count}</span>
              </Link>
            ))}
          </div>
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
          <PanelTitle icon={Play} title="Seeded Job" action="Submit" />
          <form className="formStack" onSubmit={submitSeededJob}>
            <label>
              <span>Job definition</span>
              <select value={jobDefinitionId} onChange={(event) => setJobDefinitionId(event.target.value)}>
                {seededJobs.map(([id, label]) => (
                  <option value={id} key={id}>
                    {label}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Execution pool</span>
              <select value={jobPool} onChange={(event) => setJobPool(event.target.value)}>
                {pools.map((pool) => (
                  <option value={pool} key={pool}>
                    {pool}
                  </option>
                ))}
              </select>
            </label>
            <button className="primaryAction inlineAction" disabled={isSubmitting}>
              <Send size={16} aria-hidden="true" />
              Submit seeded job
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={FileCode2} title="Python Script" action="Submit" />
          <form className="formStack" onSubmit={submitScript}>
            <label>
              <span>Execution pool</span>
              <select value={scriptPool} onChange={(event) => setScriptPool(event.target.value)}>
                {pools.map((pool) => (
                  <option value={pool} key={pool}>
                    {pool}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Script</span>
              <textarea value={script} onChange={(event) => setScript(event.target.value)} rows={8} />
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
