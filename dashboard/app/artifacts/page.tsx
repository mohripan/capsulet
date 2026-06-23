"use client";

import { Archive, Database, FileCode2, HardDrive, RefreshCw } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { Artifact, JobRun, formatBytes, getErrorMessage, listArtifacts, listRuns } from "../lib/api";

type ArtifactRow = {
  artifact: Artifact;
  run: JobRun;
};

export default function ArtifactsPage() {
  const [rows, setRows] = useState<ArtifactRow[]>([]);
  const [runs, setRuns] = useState<JobRun[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const runResponse = await listRuns({ limit: 100, sort: "created_at", direction: "desc" });
      setRuns(runResponse.runs);
      const artifactResults = await Promise.all(
        runResponse.runs.map(async (run) => {
          try {
            const response = await listArtifacts(run.id);
            return response.artifacts.map((artifact) => ({ artifact, run }));
          } catch {
            return [];
          }
        })
      );
      setRows(artifactResults.flat());
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const totalBytes = useMemo(() => rows.reduce((total, row) => total + row.artifact.size_bytes, 0), [rows]);
  const outputArtifacts = rows.filter((row) => row.artifact.kind === "artifact").length;

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Object storage"
        title="Inspect bundles, logs, and artifacts"
        description="Capsulet stores scripts, logs, and run outputs in object storage while PostgreSQL keeps metadata and references."
      />

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Archive} title="Artifacts" action={`${rows.length} live`} />
          <button className="secondaryButton workflowRefresh" onClick={refresh} disabled={loading} type="button">
            <RefreshCw size={16} aria-hidden="true" />
            {loading ? "Refreshing" : "Refresh"}
          </button>
          {error ? <div className="errorBox" role="alert">{error}</div> : null}
          <div className="artifactTable">
            {rows.map(({ artifact, run }) => (
              <article className="artifactRow" key={`${run.id}:${artifact.id}`}>
                <FileCode2 size={18} aria-hidden="true" />
                <div>
                  <h2>{artifact.name}</h2>
                  <p>{run.id} / {artifact.content_type}</p>
                </div>
                <span>{formatBytes(artifact.size_bytes)}</span>
                <span>{artifact.kind}</span>
              </article>
            ))}
          </div>
          {!loading && rows.length === 0 ? (
            <div className="emptyState">No artifacts exist yet. Run a job that writes logs or output artifacts.</div>
          ) : null}
        </section>

        <section className="panel span4">
          <PanelTitle icon={HardDrive} title="Storage Summary" action="Live API" />
          <div className="settingStack">
            <Setting label="Provider" value="Configured object store" />
            <Setting label="Recent runs scanned" value={String(runs.length)} />
            <Setting label="Output artifacts" value={String(outputArtifacts)} />
            <Setting label="Referenced size" value={formatBytes(totalBytes)} />
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Database} title="Metadata Boundary" action="Details" />
          <div className="wideNotice">
            <strong>PostgreSQL stores references, not blobs.</strong>
            <span>Script bundles, log chunks, input payloads, output payloads, and artifacts are stored in object storage.</span>
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}

function Setting({ label, value }: { label: string; value: string }) {
  return (
    <div className="settingRow">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}
