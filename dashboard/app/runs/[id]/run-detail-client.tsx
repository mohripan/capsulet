"use client";

import Link from "next/link";
import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Archive,
  ArrowLeft,
  Ban,
  Download,
  FileText,
  RefreshCw,
  TerminalSquare
} from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, StateBadge } from "../../components";
import {
  Artifact,
  JobRun,
  LogsResponse,
  cancelRun,
  downloadArtifact,
  formatBytes,
  getErrorMessage,
  getRun,
  getRunLogs,
  isTerminalStatus,
  listArtifacts
} from "../../lib/api";

export default function RunDetailClient({ id }: { id: string }) {
  const [run, setRun] = useState<JobRun | null>(null);
  const [logs, setLogs] = useState<LogsResponse | null>(null);
  const [artifacts, setArtifacts] = useState<Artifact[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isCancelling, setIsCancelling] = useState(false);
  const [downloadError, setDownloadError] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setIsLoading(true);
    setError(null);
    setDownloadError(null);
    try {
      const [nextRun, nextLogs, nextArtifacts] = await Promise.all([
        getRun(id),
        getRunLogs(id).catch(() => null),
        listArtifacts(id)
      ]);
      setRun(nextRun);
      setLogs(nextLogs);
      setArtifacts(nextArtifacts.artifacts);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }, [id]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const stdoutArtifact = useMemo(
    () => artifacts.find((artifact) => artifact.kind === "log" && artifact.name === "stdout.log"),
    [artifacts]
  );

  async function onCancel() {
    if (!run || isTerminalStatus(run.status)) {
      return;
    }
    setIsCancelling(true);
    setError(null);
    try {
      const cancelled = await cancelRun(run.id);
      setRun(cancelled);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsCancelling(false);
    }
  }

  async function onDownload(artifact: Artifact) {
    setDownloadError(null);
    try {
      await downloadArtifact(id, artifact);
    } catch (err) {
      setDownloadError(getErrorMessage(err));
    }
  }

  return (
    <DashboardShell actionLabel="Submit run">
      <PageHeader
        eyebrow="Run detail"
        title={id}
        description="Inspect the current run state, captured logs, object-backed log availability, and downloadable artifacts."
      />

      <section className="contentGrid">
        <section className="panel span12">
          <div className="detailToolbar">
            <Link className="secondaryButton" href="/runs">
              <ArrowLeft size={16} aria-hidden="true" />
              Runs
            </Link>
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              Refresh
            </button>
            {run && !isTerminalStatus(run.status) ? (
              <button className="dangerButton" onClick={onCancel} disabled={isCancelling}>
                <Ban size={16} aria-hidden="true" />
                {isCancelling ? "Cancelling" : "Cancel run"}
              </button>
            ) : null}
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {isLoading && !run ? <div className="emptyState">Loading run detail.</div> : null}
        </section>

        {run ? (
          <section className="panel span5">
            <PanelTitle icon={FileText} title="Run State" action="API" />
            <div className="detailGrid">
              <span>ID</span>
              <strong className="mono">{run.id}</strong>
              <span>Job definition</span>
              <strong>{run.job_definition_id}</strong>
              <span>Execution pool</span>
              <strong>{run.execution_pool}</strong>
              <span>Status</span>
              <StateBadge state={run.status} />
              <span>Attempts</span>
              <strong>{run.attempt_count}</strong>
            </div>
          </section>
        ) : null}

        <section className="panel span7">
          <PanelTitle icon={TerminalSquare} title="Logs" action={logs?.object_log_available ? "Object log" : "Inline"} />
          {logs?.object_log_available ? (
            <div className="wideNotice">
              <strong>Full stdout is stored as an artifact.</strong>
              <span>Download `stdout.log` from the artifacts table for the full object-backed log.</span>
            </div>
          ) : null}
          <pre className="terminal logBlock">{logs?.logs || "No logs have been captured for this run yet."}</pre>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Archive} title="Artifacts" action={`${artifacts.length} files`} />
          {downloadError ? <div className="errorBox">{downloadError}</div> : null}
          {artifacts.length === 0 ? (
            <div className="emptyState">No artifacts are recorded for this run.</div>
          ) : (
            <div className="artifactTable">
              <div className="artifactHeader">
                <span>Name</span>
                <span>Kind</span>
                <span>Size</span>
                <span>Content type</span>
                <span>Action</span>
              </div>
              {artifacts.map((artifact) => (
                <div className="artifactDetailRow" key={artifact.id}>
                  <div>
                    <strong>{artifact.name}</strong>
                    <span className="mono">{artifact.id}</span>
                  </div>
                  <span>{artifact.kind}</span>
                  <span>{formatBytes(artifact.size_bytes)}</span>
                  <span>{artifact.content_type}</span>
                  <button className="secondaryButton" onClick={() => onDownload(artifact)}>
                    <Download size={16} aria-hidden="true" />
                    Download
                  </button>
                </div>
              ))}
            </div>
          )}
          {stdoutArtifact ? (
            <div className="wideNotice compactNotice">
              <span>
                Large log artifact available as <strong>{stdoutArtifact.name}</strong>.
              </span>
            </div>
          ) : null}
        </section>
      </section>
    </DashboardShell>
  );
}
