"use client";

import Link from "next/link";
import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import { Activity, Check, FileText, Play, PlugZap, RefreshCw, X } from "lucide-react";

import {
  CreateIngestionConnectorRequest,
  IngestionConnector,
  IngestionRun,
  IngestionRunWithOutputs,
  ReviewClaim,
  approveReviewClaim,
  createIngestionConnector,
  getErrorMessage,
  listIngestionConnectors,
  listIngestionRuns,
  listReviewClaims,
  rejectReviewClaim,
  runIngestionConnector
} from "../../lib/api";

type IngestionData = {
  connectors: IngestionConnector[];
  runs: IngestionRun[];
  reviewClaims: ReviewClaim[];
};

type ReviewStatusFilter = "candidate" | "active" | "rejected";

const starterContent = `# Project Atlas

- Project Atlas is blocked by Legal Review
- Sarah approved Project Atlas`;

export default function MemoryIngestionPage() {
  const [data, setData] = useState<IngestionData>({ connectors: [], runs: [], reviewClaims: [] });
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);
  const [runningId, setRunningId] = useState("");
  const [reviewingId, setReviewingId] = useState("");
  const [reviewStatus, setReviewStatus] = useState<ReviewStatusFilter>("candidate");
  const [error, setError] = useState("");
  const [lastRun, setLastRun] = useState<IngestionRunWithOutputs | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError("");
    try {
      const [connectors, runs, reviewClaims] = await Promise.all([
        listIngestionConnectors(),
        listIngestionRuns(),
        listReviewClaims(reviewStatus)
      ]);
      setData({ connectors: connectors.connectors, runs: runs.runs, reviewClaims: reviewClaims.claims });
    } catch (error) {
      setError(getErrorMessage(error));
    } finally {
      setLoading(false);
    }
  }, [reviewStatus]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  async function createConnector(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setSubmitting(true);
    setError("");
    const form = new FormData(event.currentTarget);
    const request: CreateIngestionConnectorRequest = {
      id: String(form.get("id") || "").trim() || undefined,
      name: String(form.get("name") || "").trim(),
      kind: "local_text",
      enabled: form.get("enabled") === "on",
      config: {
        title: String(form.get("title") || "").trim(),
        content: String(form.get("content") || ""),
        content_type: String(form.get("content_type") || "text/markdown"),
        uri: String(form.get("uri") || "").trim() || undefined,
        authority: String(form.get("authority") || "medium") as "low" | "medium" | "high"
      }
    };
    try {
      await createIngestionConnector(request);
      event.currentTarget.reset();
      await refresh();
    } catch (error) {
      setError(getErrorMessage(error));
    } finally {
      setSubmitting(false);
    }
  }

  async function runConnector(id: string) {
    setRunningId(id);
    setError("");
    try {
      const result = await runIngestionConnector(id);
      setLastRun(result);
      await refresh();
    } catch (error) {
      setError(getErrorMessage(error));
    } finally {
      setRunningId("");
    }
  }

  async function reviewClaim(id: string, action: "approve" | "reject") {
    setReviewingId(id);
    setError("");
    try {
      if (action === "approve") {
        await approveReviewClaim(id);
      } else {
        await rejectReviewClaim(id);
      }
      await refresh();
    } catch (error) {
      setError(getErrorMessage(error));
    } finally {
      setReviewingId("");
    }
  }

  const latestRunsByConnector = useMemo(() => {
    const latest = new Map<string, IngestionRun>();
    for (const run of data.runs) {
      if (!latest.has(run.connector_id)) latest.set(run.connector_id, run);
    }
    return latest;
  }, [data.runs]);

  const totalClaims = data.reviewClaims.filter((claim) => claim.status === "candidate").length;
  const totalEvidence = data.runs.reduce((sum, run) => sum + run.evidence_count, 0);

  return (
    <div className="p-4 md:p-5">
      <header className="mb-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-docker-300">
            <PlugZap size={15} aria-hidden="true" />
            Memory Studio
          </div>
          <h1 className="text-2xl font-semibold tracking-normal text-slate-50">Connector Ingestion</h1>
          <p className="mt-1 max-w-3xl text-sm leading-6 text-capsulet-muted">
            Register connector sources, run extraction, and turn raw local content into candidate sources, evidence, entities, and claims.
          </p>
        </div>
        <button
          className="inline-flex items-center justify-center gap-2 rounded-md border border-capsulet-line bg-capsulet-panel px-3 py-2 text-sm font-semibold text-slate-100 hover:border-docker-500"
          onClick={() => void refresh()}
          type="button"
        >
          <RefreshCw size={17} aria-hidden="true" />
          Refresh
        </button>
      </header>

      <div className="mb-4 flex flex-wrap gap-2 border-b border-capsulet-line">
        <StudioTab href="/memory">Explore</StudioTab>
        <StudioTab href="/memory/ingestion">Ingestion</StudioTab>
        <StudioTab href="/memory/subgraphs">Subgraphs</StudioTab>
        <StudioTab href="/memory/entities">Entities</StudioTab>
        <StudioTab href="/memory/edges">Boundary edges</StudioTab>
        <StudioTab href="/memory/traces">Summary traces</StudioTab>
      </div>

      {error ? (
        <div className="mb-4 rounded-md border border-red-400/40 bg-red-950/30 px-3 py-2 text-sm text-red-100">
          {error}
        </div>
      ) : null}

      <section className="mb-4 grid gap-3 md:grid-cols-3">
        <Metric label="Connectors" value={String(data.connectors.length)} />
        <Metric label="Review queue" value={String(totalClaims)} />
        <Metric label="Evidence chunks" value={String(totalEvidence)} />
      </section>

      <div className="grid gap-4 xl:grid-cols-[minmax(340px,430px)_minmax(0,1fr)]">
        <section className="rounded-md border border-capsulet-line bg-capsulet-shell">
          <div className="flex h-11 items-center justify-between border-b border-capsulet-subtle px-3">
            <h2 className="text-sm font-semibold text-slate-50">Local Text Connector</h2>
            <span className="text-xs text-capsulet-muted">deterministic adapter</span>
          </div>
          <form className="grid gap-3 p-3" onSubmit={(event) => void createConnector(event)}>
            <TextField label="Connector ID" name="id" placeholder="connector_project_notes" />
            <TextField label="Name" name="name" placeholder="Project notes" required />
            <TextField label="Title" name="title" placeholder="Project Atlas Notes" required />
            <TextField label="URI" name="uri" placeholder="local://project-atlas.md" />
            <label className="grid gap-1 text-xs font-semibold text-capsulet-muted">
              Content type
              <select className="rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-sm text-capsulet-text outline-none" name="content_type" defaultValue="text/markdown">
                <option value="text/markdown">text/markdown</option>
                <option value="text/plain">text/plain</option>
                <option value="application/json">application/json</option>
              </select>
            </label>
            <label className="grid gap-1 text-xs font-semibold text-capsulet-muted">
              Authority
              <select className="rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-sm text-capsulet-text outline-none" name="authority" defaultValue="medium">
                <option value="low">low</option>
                <option value="medium">medium</option>
                <option value="high">high</option>
              </select>
            </label>
            <label className="grid gap-1 text-xs font-semibold text-capsulet-muted">
              Content
              <textarea
                className="min-h-[170px] resize-y rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-sm text-capsulet-text outline-none"
                name="content"
                defaultValue={starterContent}
                required
              />
            </label>
            <label className="flex items-center gap-2 text-sm text-capsulet-muted">
              <input className="accent-docker-500" defaultChecked name="enabled" type="checkbox" />
              Enabled
            </label>
            <button
              className="inline-flex items-center justify-center gap-2 rounded-md bg-docker-500 px-3 py-2 text-sm font-semibold text-white hover:bg-docker-600 disabled:cursor-not-allowed disabled:opacity-60"
              disabled={submitting}
              type="submit"
            >
              <FileText size={17} aria-hidden="true" />
              {submitting ? "Saving" : "Create connector"}
            </button>
          </form>
        </section>

        <section className="rounded-md border border-capsulet-line bg-capsulet-shell">
          <div className="flex h-11 items-center justify-between border-b border-capsulet-subtle px-3">
            <h2 className="text-sm font-semibold text-slate-50">Connectors</h2>
            <span className="text-xs text-capsulet-muted">{loading ? "loading" : "live API"}</span>
          </div>
          <div className="grid gap-2 p-3">
            {!loading && data.connectors.length === 0 ? (
              <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-4 text-sm text-capsulet-muted">
                No connectors registered yet.
              </div>
            ) : null}
            {data.connectors.map((connector) => {
              const latestRun = latestRunsByConnector.get(connector.id);
              return (
                <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={connector.id}>
                  <div className="flex flex-col gap-3 md:flex-row md:items-start md:justify-between">
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <strong className="text-sm text-slate-50">{connector.name}</strong>
                        <Badge>{connector.enabled ? "enabled" : "disabled"}</Badge>
                        <Badge>{connector.config.authority}</Badge>
                      </div>
                      <div className="mt-1 truncate text-xs text-capsulet-muted">{connector.id}</div>
                      <div className="mt-2 grid gap-1 text-xs text-capsulet-muted md:grid-cols-2">
                        <span>title: {connector.config.title}</span>
                        <span>type: {connector.config.content_type}</span>
                        <span className="md:col-span-2">uri: {connector.config.uri ?? "inline content"}</span>
                      </div>
                    </div>
                    <button
                      className="inline-flex shrink-0 items-center justify-center gap-2 rounded-md border border-docker-700 bg-docker-900/70 px-3 py-2 text-sm font-semibold text-docker-100 hover:border-docker-400 disabled:cursor-not-allowed disabled:opacity-60"
                      disabled={!connector.enabled || runningId === connector.id}
                      onClick={() => void runConnector(connector.id)}
                      type="button"
                    >
                      <Play size={16} aria-hidden="true" />
                      {runningId === connector.id ? "Running" : "Run"}
                    </button>
                  </div>
                  {latestRun ? (
                    <div className="mt-3 grid gap-2 text-xs text-capsulet-muted sm:grid-cols-4">
                      <RunCount label="status" value={latestRun.status} />
                      <RunCount label="claims" value={String(latestRun.claim_count)} />
                      <RunCount label="evidence" value={String(latestRun.evidence_count)} />
                      <RunCount label="entities" value={String(latestRun.entity_count)} />
                    </div>
                  ) : null}
                </div>
              );
            })}
          </div>
        </section>
      </div>

      <section className="mt-4 rounded-md border border-capsulet-line bg-capsulet-shell">
        <div className="flex min-h-11 flex-col gap-2 border-b border-capsulet-subtle px-3 py-2 md:flex-row md:items-center md:justify-between">
          <div>
            <h2 className="text-sm font-semibold text-slate-50">Claim Review Inbox</h2>
            <p className="mt-0.5 text-xs text-capsulet-muted">Approve or reject extracted claims before agents treat them as trusted memory.</p>
          </div>
          <div className="flex flex-wrap gap-1">
            {(["candidate", "active", "rejected"] as const).map((status) => (
              <button
                className={reviewStatus === status ? "rounded-md bg-docker-700 px-2.5 py-1.5 text-xs font-semibold text-white" : "rounded-md border border-capsulet-line bg-capsulet-panel px-2.5 py-1.5 text-xs font-semibold text-capsulet-muted hover:text-white"}
                key={status}
                onClick={() => setReviewStatus(status)}
                type="button"
              >
                {status}
              </button>
            ))}
          </div>
        </div>
        <div className="grid gap-2 p-3">
          {!loading && data.reviewClaims.length === 0 ? (
            <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-4 text-sm text-capsulet-muted">
              No {reviewStatus} claims in this project.
            </div>
          ) : null}
          {data.reviewClaims.map((claim) => (
            <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={claim.id}>
              <div className="flex flex-col gap-3 lg:flex-row lg:items-start lg:justify-between">
                <div className="min-w-0">
                  <div className="flex flex-wrap items-center gap-2">
                    <strong className="text-sm text-slate-50">{claim.predicate}</strong>
                    <Badge>{claim.status}</Badge>
                    <Badge>{claim.authority}</Badge>
                    <Badge>{Math.round(claim.confidence * 100)}%</Badge>
                  </div>
                  <div className="mt-2 text-sm text-slate-200">{claim.object}</div>
                  <div className="mt-2 grid gap-1 text-xs text-capsulet-muted md:grid-cols-2">
                    <span className="truncate">claim: {claim.id}</span>
                    <span className="truncate">subject: {claim.subject_id}</span>
                    <span className="truncate md:col-span-2">evidence: {claim.evidence_ids.length ? claim.evidence_ids.join(", ") : "none"}</span>
                  </div>
                  <div className="mt-3 grid gap-2">
                    {claim.evidence.length === 0 ? (
                      <div className="rounded-md border border-amber-500/30 bg-amber-950/20 p-3 text-xs text-amber-100">
                        No evidence record is available for this claim.
                      </div>
                    ) : null}
                    {claim.evidence.map((evidence) => {
                      const source = claim.sources.find((source) => source.id === evidence.source_id);
                      return (
                        <div className="rounded-md border border-capsulet-subtle bg-capsulet-bg p-3" key={evidence.id}>
                          <div className="mb-2 flex flex-wrap items-center gap-2">
                            <Badge>{evidence.locator}</Badge>
                            <span className="text-xs text-capsulet-muted">{evidence.observed_at}</span>
                          </div>
                          <p className="text-sm leading-6 text-slate-100">{evidence.excerpt}</p>
                          <div className="mt-2 grid gap-1 text-xs text-capsulet-muted md:grid-cols-2">
                            <span className="truncate">source: {source?.title ?? evidence.source_id}</span>
                            <span className="truncate">authority: {source?.authority ?? "unknown"}</span>
                            <span className="truncate md:col-span-2">uri: {source?.uri ?? "inline source"}</span>
                          </div>
                        </div>
                      );
                    })}
                  </div>
                </div>
                {claim.status === "candidate" ? (
                  <div className="flex shrink-0 gap-2">
                    <button
                      className="inline-flex items-center justify-center gap-2 rounded-md border border-docker-600 bg-docker-900/70 px-3 py-2 text-sm font-semibold text-docker-100 hover:border-docker-300 disabled:cursor-not-allowed disabled:opacity-60"
                      disabled={reviewingId === claim.id}
                      onClick={() => void reviewClaim(claim.id, "approve")}
                      type="button"
                    >
                      <Check size={16} aria-hidden="true" />
                      Approve
                    </button>
                    <button
                      className="inline-flex items-center justify-center gap-2 rounded-md border border-red-500/50 bg-red-950/30 px-3 py-2 text-sm font-semibold text-red-100 hover:border-red-300 disabled:cursor-not-allowed disabled:opacity-60"
                      disabled={reviewingId === claim.id}
                      onClick={() => void reviewClaim(claim.id, "reject")}
                      type="button"
                    >
                      <X size={16} aria-hidden="true" />
                      Reject
                    </button>
                  </div>
                ) : null}
              </div>
            </div>
          ))}
        </div>
      </section>

      <section className="mt-4 rounded-md border border-capsulet-line bg-capsulet-shell">
        <div className="flex h-11 items-center justify-between border-b border-capsulet-subtle px-3">
          <h2 className="text-sm font-semibold text-slate-50">Recent Ingestion Runs</h2>
          <Activity size={17} className="text-docker-300" aria-hidden="true" />
        </div>
        <div className="grid gap-2 p-3">
          {lastRun ? (
            <div className="rounded-md border border-docker-700 bg-docker-950/30 p-3 text-sm text-docker-100">
              Last run generated {lastRun.outputs.claims.length} claim output(s), {lastRun.outputs.evidence.length} evidence output(s), and {lastRun.outputs.entities.length} entity output(s).
            </div>
          ) : null}
          {!loading && data.runs.length === 0 ? (
            <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-4 text-sm text-capsulet-muted">
              No ingestion runs yet.
            </div>
          ) : null}
          {data.runs.slice(0, 8).map((run) => (
            <div className="grid gap-2 rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3 text-sm md:grid-cols-[minmax(0,1.6fr)_repeat(5,minmax(70px,1fr))]" key={run.id}>
              <div className="min-w-0">
                <strong className="block truncate text-slate-50">{run.id}</strong>
                <span className="block truncate text-xs text-capsulet-muted">{run.connector_id}</span>
              </div>
              <RunCount label="status" value={run.status} />
              <RunCount label="sources" value={String(run.source_count)} />
              <RunCount label="evidence" value={String(run.evidence_count)} />
              <RunCount label="entities" value={String(run.entity_count)} />
              <RunCount label="claims" value={String(run.claim_count)} />
            </div>
          ))}
        </div>
      </section>
    </div>
  );
}

function StudioTab({ href, children }: { href: string; children: React.ReactNode }) {
  return <Link className="border-b-2 border-transparent px-3 py-2 text-sm text-capsulet-muted hover:border-docker-500 hover:text-docker-100" href={href}>{children}</Link>;
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-capsulet-line bg-capsulet-shell p-3">
      <span className="text-xs text-capsulet-muted">{label}</span>
      <strong className="mt-1 block text-2xl font-semibold text-slate-50">{value}</strong>
    </div>
  );
}

function TextField({ label, name, placeholder, required = false }: { label: string; name: string; placeholder: string; required?: boolean }) {
  return (
    <label className="grid gap-1 text-xs font-semibold text-capsulet-muted">
      {label}
      <input
        className="rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-sm text-capsulet-text outline-none"
        name={name}
        placeholder={placeholder}
        required={required}
      />
    </label>
  );
}

function Badge({ children }: { children: React.ReactNode }) {
  return (
    <span className="rounded-md border border-docker-700 bg-docker-950/40 px-2 py-0.5 text-[11px] font-semibold text-docker-200">
      {children}
    </span>
  );
}

function RunCount({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0 rounded-md border border-capsulet-subtle bg-capsulet-bg px-2 py-1.5">
      <span className="block text-[11px] uppercase text-capsulet-muted">{label}</span>
      <strong className="block truncate text-xs text-slate-100">{value}</strong>
    </div>
  );
}
