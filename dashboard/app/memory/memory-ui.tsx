"use client";

import Link from "next/link";
import { FormEvent, ReactNode, useCallback, useEffect, useMemo, useState } from "react";
import {
  ArrowRight,
  Check,
  CircleDotDashed,
  DatabaseZap,
  GitBranch,
  Layers3,
  Network,
  Plus,
  RefreshCw,
  ShieldCheck
} from "lucide-react";
import {
  CanonicalEntity,
  ClaimConflict,
  CreateSubgraphEdgeRequest,
  EntityResolution,
  MemorySubgraph,
  SubgraphEdge,
  SummaryTrace,
  activateMemorySubgraph,
  confirmEntityResolution,
  createCanonicalEntity,
  createEntityGraphAttachment,
  createMemorySubgraph,
  createSubgraphEdge,
  createSummaryTrace,
  dismissClaimConflict,
  getErrorMessage,
  listCanonicalEntities,
  listClaimConflicts,
  listEntityResolutions,
  listMemorySubgraphs,
  rejectEntityResolution,
  resolveClaimConflict
} from "../lib/api";

type MemoryData = {
  subgraphs: MemorySubgraph[];
  canonicalEntities: CanonicalEntity[];
  entityResolutions: EntityResolution[];
  claimConflicts: ClaimConflict[];
};

const fieldClass =
  "w-full rounded-md border border-capsulet-line bg-capsulet-bg px-3 py-2 text-sm text-capsulet-text outline-none placeholder:text-capsulet-muted focus:border-docker-500";
const buttonClass =
  "inline-flex items-center justify-center gap-2 rounded-md bg-docker-500 px-3 py-2 text-sm font-semibold text-white hover:bg-docker-600 disabled:cursor-not-allowed disabled:opacity-60";
const subtleButtonClass =
  "inline-flex items-center justify-center gap-2 rounded-md border border-capsulet-line bg-capsulet-panel px-3 py-2 text-sm font-semibold text-capsulet-text hover:border-docker-700";

export function MemoryWorkbenchPage() {
  const { data, errors, loading, refresh } = useMemoryData();
  const [pendingConflict, setPendingConflict] = useState("");
  const [message, setMessage] = useState("");
  const selected = data.subgraphs[0];
  const activeCount = data.subgraphs.filter((subgraph) => subgraph.status === "active").length;
  const draftCount = Math.max(0, data.subgraphs.length - activeCount);
  const graphNodes = data.subgraphs.length ? data.subgraphs.slice(0, 5) : demoSubgraphs;
  const entities = data.canonicalEntities.length ? data.canonicalEntities.slice(0, 4) : demoEntities;

  async function reviewConflict(conflict: ClaimConflict, action: "resolve" | "dismiss") {
    setPendingConflict(conflict.id);
    setMessage("");
    try {
      if (action === "resolve") {
        const preferredClaimId = conflict.claim_ids[conflict.claim_ids.length - 1];
        await resolveClaimConflict(conflict.id, preferredClaimId);
        setMessage("Conflict resolved.");
      } else {
        await dismissClaimConflict(conflict.id);
        setMessage("Conflict dismissed.");
      }
      await refresh();
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPendingConflict("");
    }
  }

  return (
    <MemoryPageFrame
      eyebrow="Memory Studio"
      title="Graph Workbench"
      description="Inspect nested memory contexts, governed summaries, canonical entities, and explicit cross-subgraph boundaries."
      action={<RefreshButton loading={loading} onClick={refresh} />}
    >
      <ErrorList errors={errors} />
      <section className="grid gap-3 xl:grid-cols-[minmax(0,1.55fr)_380px]">
        <Panel className="min-h-[470px]" title="Nested Memory Graph" meta={selected ? selected.id : "sample topology"}>
          <GraphCanvas subgraphs={graphNodes} entities={entities} />
        </Panel>
        <div className="grid content-start gap-3">
          <Panel title="Memory Health" meta={loading ? "loading" : "live API"}>
            <div className="grid grid-cols-2 gap-2 p-3 sm:grid-cols-4 xl:grid-cols-2">
              <Metric label="Subgraphs" value={String(data.subgraphs.length)} />
              <Metric label="Active" value={String(activeCount)} />
              <Metric label="Drafts" value={String(draftCount)} />
              <Metric label="Entities" value={String(data.canonicalEntities.length)} />
            </div>
          </Panel>
          <Panel title="Selected Subgraph" meta={selected?.name ?? "No subgraph"}>
            {selected ? <SubgraphInspector subgraph={selected} /> : <EmptyState title="No subgraphs yet" body="Create a subgraph to define a bounded memory module." href="/memory/subgraphs" action="Create subgraph" />}
          </Panel>
        </div>
        <Panel title="Claim Review Inbox" meta="governance">
          <div className="grid gap-2 p-3 text-sm">
            {data.claimConflicts.map((conflict) => (
              <article className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={conflict.id}>
                <div className="flex flex-wrap items-start justify-between gap-3">
                  <div className="min-w-0">
                    <strong className="block text-slate-100">{conflict.predicate}</strong>
                    <p className="mt-1 text-xs text-capsulet-muted">{conflict.reason}</p>
                    <p className="mt-1 text-xs text-capsulet-muted">claims: {conflict.claim_ids.join(", ")}</p>
                  </div>
                  <Badge>{conflict.status}</Badge>
                </div>
                <div className="mt-3 flex flex-wrap gap-2">
                  <button className={subtleButtonClass} disabled={pendingConflict === conflict.id} onClick={() => reviewConflict(conflict, "resolve")} type="button">
                    <Check size={15} />Resolve
                  </button>
                  <button className={subtleButtonClass} disabled={pendingConflict === conflict.id} onClick={() => reviewConflict(conflict, "dismiss")} type="button">
                    Dismiss
                  </button>
                </div>
              </article>
            ))}
            {!data.claimConflicts.length ? <EmptyState title="No candidate conflicts" body="Approve conflicting claims to populate the governance inbox." /> : null}
            <FormMessage message={message} />
          </div>
        </Panel>
        <Panel title="Entity Resolution" meta="canonical identity">
          <div className="grid gap-2 p-3">
            {entities.map((entity) => (
              <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={entity.id}>
                <div className="flex items-center justify-between gap-3">
                  <strong className="text-sm text-slate-100">{entity.display_name}</strong>
                  <Badge>{entity.entity_type}</Badge>
                </div>
                <p className="mt-1 truncate text-xs text-capsulet-muted">{entity.aliases.join(", ") || "No aliases"}</p>
              </div>
            ))}
          </div>
        </Panel>
      </section>
    </MemoryPageFrame>
  );
}

export function MemorySubgraphsPage() {
  const { data, errors, loading, refresh } = useMemoryData();
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setMessage("");
    const form = new FormData(event.currentTarget);
    try {
      await createMemorySubgraph({
        id: optionalString(form, "id"),
        parent_subgraph_id: optionalString(form, "parent_subgraph_id"),
        name: requiredString(form, "name"),
        description: optionalString(form, "description")
      });
      event.currentTarget.reset();
      setMessage("Subgraph created.");
      await refresh();
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  async function activate(subgraph: MemorySubgraph) {
    const contractId = window.prompt("Contract ID for this subgraph");
    const summaryClaimId = window.prompt("Summary claim ID for this subgraph");
    if (!contractId || !summaryClaimId) return;
    setPending(true);
    setMessage("");
    try {
      await activateMemorySubgraph(subgraph.id, {
        owner_kind: "team",
        owner_id: subgraph.owner_id || "memory-team",
        contract_id: contractId,
        permissions: { visibility: "restricted", summary_visible: true },
        summary_claim_id: summaryClaimId
      });
      setMessage("Subgraph activated.");
      await refresh();
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <MemoryPageFrame eyebrow="Memory Studio" title="Subgraphs" description="Create bounded memory modules with owners, schemas, permissions, and summary nodes.">
      <ErrorList errors={errors} />
      <section className="grid gap-3 xl:grid-cols-[380px_minmax(0,1fr)]">
        <Panel title="Create Subgraph" meta="bounded context">
          <form className="grid gap-3 p-3" onSubmit={submit}>
            <Field label="ID" name="id" placeholder="graph_project_atlas" />
            <Field label="Name" name="name" placeholder="Project Atlas" required />
            <Field label="Description" name="description" placeholder="Governed project memory" />
            <Field label="Parent subgraph ID" name="parent_subgraph_id" placeholder="company_memory" />
            <button className={buttonClass} disabled={pending} type="submit"><Plus size={16} />Create subgraph</button>
            <FormMessage message={message} />
          </form>
        </Panel>
        <Panel title="Subgraph Registry" meta={loading ? "loading" : `${data.subgraphs.length} records`}>
          <div className="overflow-x-auto">
            <table className="w-full min-w-[760px] text-left text-sm">
              <thead className="text-[11px] uppercase tracking-wider text-capsulet-muted">
                <tr><th className="px-3 py-2">Name</th><th className="px-3 py-2">Owner</th><th className="px-3 py-2">Schema</th><th className="px-3 py-2">Summary</th><th className="px-3 py-2">Status</th><th className="px-3 py-2" /></tr>
              </thead>
              <tbody className="divide-y divide-capsulet-subtle">
                {data.subgraphs.map((subgraph) => (
                  <tr key={subgraph.id}>
                    <td className="px-3 py-3"><strong className="block text-slate-100">{subgraph.name}</strong><span className="text-xs text-capsulet-muted">{subgraph.id}</span></td>
                    <td className="px-3 py-3 text-capsulet-muted">{subgraph.owner_id ?? "Missing"}</td>
                    <td className="px-3 py-3 text-capsulet-muted">{subgraph.contract_id ?? "Missing"}</td>
                    <td className="px-3 py-3 text-capsulet-muted">{subgraph.summary_claim_id ?? "Missing"}</td>
                    <td className="px-3 py-3"><Badge>{subgraph.status}</Badge></td>
                    <td className="px-3 py-3 text-right">
                      <button className={subtleButtonClass} disabled={pending || subgraph.status === "active"} onClick={() => activate(subgraph)} type="button">
                        <Check size={15} />Activate
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
            {!data.subgraphs.length ? <EmptyState title="No subgraphs" body="Create the first bounded memory context." /> : null}
          </div>
        </Panel>
      </section>
    </MemoryPageFrame>
  );
}

export function MemoryEntitiesPage() {
  const { data, errors, loading, refresh } = useMemoryData();
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setMessage("");
    const form = new FormData(event.currentTarget);
    try {
      const entity = await createCanonicalEntity({
        id: optionalString(form, "id"),
        entity_type: requiredString(form, "entity_type"),
        display_name: requiredString(form, "display_name"),
        aliases: splitCsv(optionalString(form, "aliases"))
      });
      const attachTo = optionalString(form, "attach_to_subgraph_id");
      if (attachTo) {
        await createEntityGraphAttachment({
          canonical_entity_id: entity.id,
          subgraph_id: attachTo,
          attachment_type: "primary"
        });
      }
      event.currentTarget.reset();
      setMessage("Canonical entity saved.");
      await refresh();
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  async function reviewResolution(id: string, action: "confirm" | "reject") {
    setPending(true);
    setMessage("");
    try {
      if (action === "confirm") {
        await confirmEntityResolution(id);
        setMessage("Entity resolution confirmed.");
      } else {
        await rejectEntityResolution(id);
        setMessage("Entity resolution rejected.");
      }
      await refresh();
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <MemoryPageFrame eyebrow="Memory Studio" title="Entities" description="Manage shared canonical identities that can appear inside many bounded subgraphs.">
      <ErrorList errors={errors} />
      <section className="grid gap-3 xl:grid-cols-[380px_minmax(0,1fr)]">
        <Panel title="Create Canonical Entity" meta="shared identity">
          <form className="grid gap-3 p-3" onSubmit={submit}>
            <Field label="ID" name="id" placeholder="canonical_customer_a" />
            <Field label="Entity type" name="entity_type" placeholder="Customer" required />
            <Field label="Display name" name="display_name" placeholder="Customer A" required />
            <Field label="Aliases" name="aliases" placeholder="customer-a, ACME" />
            <SelectField label="Attach to subgraph" name="attach_to_subgraph_id" options={data.subgraphs.map((subgraph) => [subgraph.id, subgraph.name])} />
            <button className={buttonClass} disabled={pending} type="submit"><Plus size={16} />Save entity</button>
            <FormMessage message={message} />
          </form>
        </Panel>
        <Panel title="Canonical Entity Registry" meta={loading ? "loading" : `${data.canonicalEntities.length} records`}>
          <div className="grid gap-2 p-3 md:grid-cols-2">
            {data.canonicalEntities.map((entity) => (
              <article className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={entity.id}>
                <div className="flex items-center justify-between gap-3">
                  <strong className="text-slate-100">{entity.display_name}</strong>
                  <Badge>{entity.entity_type}</Badge>
                </div>
                <p className="mt-1 text-xs text-capsulet-muted">{entity.id}</p>
                <p className="mt-3 text-sm text-slate-300">{entity.aliases.join(", ") || "No aliases recorded"}</p>
              </article>
            ))}
            {!data.canonicalEntities.length ? <EmptyState title="No canonical entities" body="Create a shared identity before resolving local entities." /> : null}
          </div>
        </Panel>
        <Panel title="Resolution Queue" meta={`${data.entityResolutions.length} candidates`}>
          <div className="grid gap-2 p-3">
            {data.entityResolutions.map((resolution) => {
              const canonical = data.canonicalEntities.find((entity) => entity.id === resolution.canonical_entity_id);
              return (
                <article className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3" key={resolution.id}>
                  <div className="flex flex-wrap items-start justify-between gap-3">
                    <div className="min-w-0">
                      <strong className="block text-sm text-slate-100">{canonical?.display_name ?? resolution.canonical_entity_id}</strong>
                      <p className="mt-1 text-xs text-capsulet-muted">local entity: {resolution.entity_id}</p>
                      <p className="mt-1 text-xs text-capsulet-muted">evidence: {resolution.evidence_ids.join(", ") || "none"}</p>
                    </div>
                    <Badge>{Math.round(resolution.confidence * 100)}%</Badge>
                  </div>
                  <div className="mt-3 flex flex-wrap gap-2">
                    <button className={subtleButtonClass} disabled={pending} onClick={() => reviewResolution(resolution.id, "confirm")} type="button">
                      <Check size={15} />Confirm
                    </button>
                    <button className={subtleButtonClass} disabled={pending} onClick={() => reviewResolution(resolution.id, "reject")} type="button">
                      Reject
                    </button>
                  </div>
                </article>
              );
            })}
            {!data.entityResolutions.length ? <EmptyState title="No resolution candidates" body="Run ingestion after creating canonical identities to generate candidate matches." /> : null}
          </div>
        </Panel>
      </section>
    </MemoryPageFrame>
  );
}

export function MemoryEdgesPage() {
  const { data, errors } = useMemoryData();
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  const [created, setCreated] = useState<SubgraphEdge | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setMessage("");
    const form = new FormData(event.currentTarget);
    const request: CreateSubgraphEdgeRequest = {
      id: optionalString(form, "id"),
      edge_type: requiredString(form, "edge_type"),
      from_subgraph_id: requiredString(form, "from_subgraph_id"),
      to_subgraph_id: requiredString(form, "to_subgraph_id"),
      from_member_kind: requiredString(form, "from_member_kind"),
      from_member_id: requiredString(form, "from_member_id"),
      to_member_kind: requiredString(form, "to_member_kind"),
      to_member_id: requiredString(form, "to_member_id"),
      claim_ids: splitCsv(optionalString(form, "claim_ids")),
      evidence_ids: splitCsv(optionalString(form, "evidence_ids"))
    };
    try {
      setCreated(await createSubgraphEdge(request));
      event.currentTarget.reset();
      setMessage("Cross-subgraph edge created.");
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <MemoryPageFrame eyebrow="Memory Studio" title="Explicit Boundary Edges" description="Create auditable relationships that cross one bounded memory context into another.">
      <ErrorList errors={errors} />
      <section className="grid gap-3 xl:grid-cols-[420px_minmax(0,1fr)]">
        <Panel title="Create Boundary Edge" meta="cross-subgraph">
          <form className="grid gap-3 p-3" onSubmit={submit}>
            <Field label="ID" name="id" placeholder="edge_sales_engineering" />
            <Field label="Edge type" name="edge_type" placeholder="contradicts" required />
            <SelectField label="From subgraph" name="from_subgraph_id" options={data.subgraphs.map((subgraph) => [subgraph.id, subgraph.name])} required />
            <SelectField label="To subgraph" name="to_subgraph_id" options={data.subgraphs.map((subgraph) => [subgraph.id, subgraph.name])} required />
            <Field label="From member kind" name="from_member_kind" placeholder="claim" required />
            <Field label="From member ID" name="from_member_id" placeholder="claim_sales" required />
            <Field label="To member kind" name="to_member_kind" placeholder="claim" required />
            <Field label="To member ID" name="to_member_id" placeholder="claim_engineering" required />
            <Field label="Claim IDs" name="claim_ids" placeholder="claim_sales, claim_engineering" />
            <Field label="Evidence IDs" name="evidence_ids" placeholder="evidence_1" />
            <button className={buttonClass} disabled={pending} type="submit"><GitBranch size={16} />Create edge</button>
            <FormMessage message={message} />
          </form>
        </Panel>
        <Panel title="Last Created Edge" meta="audit result">
          {created ? <JsonBlock value={created} /> : <EmptyState title="No edge created in this session" body="Create an edge to inspect the persisted boundary object." />}
        </Panel>
      </section>
    </MemoryPageFrame>
  );
}

export function MemoryTracesPage() {
  const { data, errors } = useMemoryData();
  const [pending, setPending] = useState(false);
  const [message, setMessage] = useState("");
  const [created, setCreated] = useState<SummaryTrace | null>(null);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setPending(true);
    setMessage("");
    const form = new FormData(event.currentTarget);
    try {
      setCreated(await createSummaryTrace({
        subgraph_id: requiredString(form, "subgraph_id"),
        summary_claim_id: requiredString(form, "summary_claim_id"),
        inner_claim_ids: splitCsv(optionalString(form, "inner_claim_ids")),
        evidence_ids: splitCsv(optionalString(form, "evidence_ids"))
      }));
      event.currentTarget.reset();
      setMessage("Summary trace created.");
    } catch (error) {
      setMessage(getErrorMessage(error));
    } finally {
      setPending(false);
    }
  }

  return (
    <MemoryPageFrame eyebrow="Memory Studio" title="Summary Traces" description="Link parent-visible summaries back to inner claims and evidence so compressed memory remains auditable.">
      <ErrorList errors={errors} />
      <section className="grid gap-3 xl:grid-cols-[420px_minmax(0,1fr)]">
        <Panel title="Create Summary Trace" meta="summary to claims">
          <form className="grid gap-3 p-3" onSubmit={submit}>
            <SelectField label="Subgraph" name="subgraph_id" options={data.subgraphs.map((subgraph) => [subgraph.id, subgraph.name])} required />
            <Field label="Summary claim ID" name="summary_claim_id" placeholder="claim_summary" required />
            <Field label="Inner claim IDs" name="inner_claim_ids" placeholder="claim_1, claim_2" />
            <Field label="Evidence IDs" name="evidence_ids" placeholder="evidence_1, evidence_2" />
            <button className={buttonClass} disabled={pending} type="submit"><CircleDotDashed size={16} />Create trace</button>
            <FormMessage message={message} />
          </form>
        </Panel>
        <Panel title="Last Created Trace" meta="traceability result">
          {created ? <JsonBlock value={created} /> : <EmptyState title="No trace created in this session" body="Create a trace to inspect the summary evidence contract." />}
        </Panel>
      </section>
    </MemoryPageFrame>
  );
}

export function MemoryContractsPage() {
  return (
    <MemoryPageFrame eyebrow="Memory Studio" title="Schema Studio" description="Define the memory contracts that control graph structure, extraction rules, trust behavior, and retrieval policy.">
      <Panel title="Memory Contract DSL" meta="planned editor">
        <div className="grid gap-3 p-3 lg:grid-cols-[1.1fr_.9fr]">
          <pre className="overflow-auto rounded-md bg-capsulet-canvas p-4 text-xs leading-5 text-slate-200">{`entity Project:
  fields:
    name: string
    status: enum[planned, active, blocked, completed]

claim_policy:
  require_source: true
  store_confidence: true
  allow_contradictions: true

retrieval_policy customer_support:
  expand:
    max_hops: 3`}</pre>
          <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-4">
            <h2 className="text-sm font-semibold text-slate-100">First implementation scope</h2>
            <p className="mt-2 text-sm leading-6 text-capsulet-muted">
              Capsulet already stores memory contracts through the backend API. This page establishes the Studio surface;
              a full editor with validation, diffing, and policy previews should land after the graph workbench is stable.
            </p>
            <Link className="mt-4 inline-flex items-center gap-2 text-sm font-semibold text-docker-300 hover:text-docker-200" href="/memory/subgraphs">
              Use contracts on subgraphs <ArrowRight size={15} />
            </Link>
          </div>
        </div>
      </Panel>
    </MemoryPageFrame>
  );
}

function useMemoryData() {
  const [data, setData] = useState<MemoryData>({ subgraphs: [], canonicalEntities: [], entityResolutions: [], claimConflicts: [] });
  const [errors, setErrors] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true);
    setErrors([]);
    const [subgraphResult, entityResult, resolutionResult, conflictResult] = await Promise.allSettled([
      listMemorySubgraphs(),
      listCanonicalEntities(),
      listEntityResolutions("candidate"),
      listClaimConflicts("candidate")
    ]);
    const nextErrors: string[] = [];
    setData({
      subgraphs: subgraphResult.status === "fulfilled" ? subgraphResult.value.subgraphs : [],
      canonicalEntities: entityResult.status === "fulfilled" ? entityResult.value.canonical_entities : [],
      entityResolutions: resolutionResult.status === "fulfilled" ? resolutionResult.value.entity_resolutions : [],
      claimConflicts: conflictResult.status === "fulfilled" ? conflictResult.value.conflicts : []
    });
    if (subgraphResult.status === "rejected") nextErrors.push(`Subgraphs: ${getErrorMessage(subgraphResult.reason)}`);
    if (entityResult.status === "rejected") nextErrors.push(`Canonical entities: ${getErrorMessage(entityResult.reason)}`);
    if (resolutionResult.status === "rejected") nextErrors.push(`Entity resolutions: ${getErrorMessage(resolutionResult.reason)}`);
    if (conflictResult.status === "rejected") nextErrors.push(`Claim conflicts: ${getErrorMessage(conflictResult.reason)}`);
    setErrors(nextErrors);
    setLoading(false);
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return { data, errors, loading, refresh };
}

function MemoryPageFrame({ eyebrow, title, description, action, children }: { eyebrow: string; title: string; description: string; action?: ReactNode; children: ReactNode }) {
  return (
    <div className="p-4 md:p-5">
      <header className="mb-4 flex flex-col gap-3 md:flex-row md:items-end md:justify-between">
        <div>
          <div className="mb-2 flex items-center gap-2 text-xs font-semibold uppercase tracking-wider text-docker-300">
            <Network size={15} aria-hidden="true" />
            {eyebrow}
          </div>
          <h1 className="text-2xl font-semibold tracking-normal text-slate-50">{title}</h1>
          <p className="mt-1 max-w-3xl text-sm leading-6 text-capsulet-muted">{description}</p>
        </div>
        {action}
      </header>
      <div className="mb-4 flex flex-wrap gap-2 border-b border-capsulet-line">
        <Tab href="/memory">Explore</Tab>
        <Tab href="/memory/ingestion">Ingestion</Tab>
        <Tab href="/memory/subgraphs">Subgraphs</Tab>
        <Tab href="/memory/entities">Entities</Tab>
        <Tab href="/memory/edges">Boundary edges</Tab>
        <Tab href="/memory/traces">Summary traces</Tab>
      </div>
      {children}
    </div>
  );
}

function Tab({ href, children }: { href: string; children: ReactNode }) {
  return <Link className="border-b-2 border-transparent px-3 py-2 text-sm text-capsulet-muted hover:border-docker-500 hover:text-docker-100" href={href}>{children}</Link>;
}

function Panel({ title, meta, className = "", children }: { title: string; meta?: string; className?: string; children: ReactNode }) {
  return (
    <section className={`rounded-md border border-capsulet-line bg-capsulet-shell ${className}`}>
      <div className="flex h-11 items-center justify-between border-b border-capsulet-subtle px-3">
        <h2 className="text-sm font-semibold text-slate-50">{title}</h2>
        {meta ? <span className="text-xs text-capsulet-muted">{meta}</span> : null}
      </div>
      {children}
    </section>
  );
}

function GraphCanvas({ subgraphs, entities }: { subgraphs: MemorySubgraph[]; entities: CanonicalEntity[] }) {
  const positioned = useMemo(() => {
    const points = [
      ["8%", "17%"],
      ["43%", "28%"],
      ["72%", "18%"],
      ["69%", "56%"],
      ["25%", "66%"]
    ];
    return subgraphs.map((subgraph, index) => ({ subgraph, left: points[index % points.length][0], top: points[index % points.length][1] }));
  }, [subgraphs]);

  return (
    <div className="relative h-[430px] overflow-hidden bg-capsulet-canvas">
      <div className="absolute inset-0 bg-[linear-gradient(rgba(36,150,237,0.055)_1px,transparent_1px),linear-gradient(90deg,rgba(36,150,237,0.055)_1px,transparent_1px)] bg-[size:36px_36px]" />
      <div className="absolute left-[20%] top-[25%] h-0.5 w-[30%] rotate-[18deg] bg-[repeating-linear-gradient(90deg,rgba(87,169,225,.82)_0_7px,transparent_7px_13px)]" />
      <div className="absolute left-[48%] top-[36%] h-0.5 w-[25%] rotate-[28deg] bg-[repeating-linear-gradient(90deg,rgba(87,169,225,.82)_0_7px,transparent_7px_13px)]" />
      <div className="absolute left-[38%] top-[63%] h-0.5 w-[32%] -rotate-[20deg] bg-[repeating-linear-gradient(90deg,rgba(87,169,225,.82)_0_7px,transparent_7px_13px)]" />
      {positioned.map(({ subgraph, left, top }, index) => (
        <div
          className={index === 0 ? "absolute min-w-[136px] rounded-md border border-docker-700 bg-docker-800 p-3 shadow-flat" : "absolute min-w-[136px] rounded-md border border-capsulet-line bg-capsulet-panel p-3 shadow-flat"}
          key={subgraph.id}
          style={{ left, top }}
        >
          <strong className="block text-sm text-slate-50">{subgraph.name}</strong>
          <span className="mt-1 block text-xs text-capsulet-muted">{subgraph.parent_subgraph_id ? "nested subgraph" : "root graph"}</span>
          <span className="block text-xs text-capsulet-muted">status: {subgraph.status}</span>
        </div>
      ))}
      {entities.slice(0, 2).map((entity, index) => (
        <div className="absolute rounded-md border border-capsulet-line bg-capsulet-panel p-3 shadow-flat" key={entity.id} style={{ left: `${58 + index * 14}%`, top: `${12 + index * 43}%` }}>
          <strong className="block text-sm text-slate-50">{entity.display_name}</strong>
          <span className="block text-xs text-capsulet-muted">canonical entity</span>
        </div>
      ))}
    </div>
  );
}

function SubgraphInspector({ subgraph }: { subgraph: MemorySubgraph }) {
  return (
    <div className="grid gap-2 p-3">
      <InspectorRow label="Owner" value={subgraph.owner_id ?? "Missing"} badge={subgraph.owner_kind ?? "required"} />
      <InspectorRow label="Schema" value={subgraph.contract_id ?? "Missing"} badge={subgraph.contract_id ? "valid" : "missing"} />
      <InspectorRow label="Permissions" value={subgraph.permissions ? "Configured" : "Missing"} badge="policy" />
      <InspectorRow label="Summary trace" value={subgraph.summary_claim_id ?? "Missing"} badge={subgraph.summary_claim_id ? "auditable" : "untraced"} />
    </div>
  );
}

function InspectorRow({ label, value, badge }: { label: string; value: string; badge: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3">
      <div className="min-w-0">
        <strong className="block text-sm text-slate-100">{label}</strong>
        <span className="block truncate text-xs text-capsulet-muted">{value}</span>
      </div>
      <Badge>{badge}</Badge>
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="rounded-md border border-capsulet-subtle bg-capsulet-canvas p-3">
      <span className="text-[11px] text-capsulet-muted">{label}</span>
      <strong className="mt-1 block text-xl text-slate-50">{value}</strong>
    </div>
  );
}

function ReviewRow({ claim, source, status }: { claim: string; source: string; status: string }) {
  return (
    <div className="grid grid-cols-[1fr_.7fr_auto] gap-3 py-3">
      <span className="text-slate-200">{claim}</span>
      <span className="text-capsulet-muted">{source}</span>
      <Badge>{status}</Badge>
    </div>
  );
}

function EmptyState({ title, body, href, action }: { title: string; body: string; href?: string; action?: string }) {
  return (
    <div className="p-5 text-sm">
      <strong className="block text-slate-100">{title}</strong>
      <p className="mt-1 text-capsulet-muted">{body}</p>
      {href && action ? <Link className="mt-3 inline-flex items-center gap-2 text-docker-300 hover:text-docker-200" href={href}>{action}<ArrowRight size={15} /></Link> : null}
    </div>
  );
}

function Field({ label, name, placeholder, required = false }: { label: string; name: string; placeholder?: string; required?: boolean }) {
  return (
    <label className="grid gap-1.5 text-sm text-slate-200">
      <span>{label}</span>
      <input className={fieldClass} name={name} placeholder={placeholder} required={required} />
    </label>
  );
}

function SelectField({ label, name, options, required = false }: { label: string; name: string; options: Array<[string, string]>; required?: boolean }) {
  return (
    <label className="grid gap-1.5 text-sm text-slate-200">
      <span>{label}</span>
      <select className={fieldClass} name={name} required={required}>
        <option value="">Select...</option>
        {options.map(([value, text]) => <option key={value} value={value}>{text}</option>)}
      </select>
    </label>
  );
}

function RefreshButton({ loading, onClick }: { loading: boolean; onClick: () => void }) {
  return <button className={subtleButtonClass} disabled={loading} onClick={onClick} type="button"><RefreshCw className={loading ? "animate-spin" : ""} size={16} />Refresh</button>;
}

function Badge({ children }: { children: ReactNode }) {
  return <span className="shrink-0 rounded-full bg-docker-800 px-2 py-1 text-[11px] font-medium text-docker-200">{children}</span>;
}

function ErrorList({ errors }: { errors: string[] }) {
  if (!errors.length) return null;
  return (
    <div className="mb-3 grid gap-2">
      {errors.map((error) => <div className="rounded-md border border-red-900/60 bg-red-950/40 px-3 py-2 text-sm text-red-100" key={error}>{error}</div>)}
    </div>
  );
}

function FormMessage({ message }: { message: string }) {
  if (!message) return null;
  return <p className="text-sm text-capsulet-muted" role="status">{message}</p>;
}

function JsonBlock({ value }: { value: unknown }) {
  return <pre className="m-3 overflow-auto rounded-md bg-capsulet-canvas p-3 text-xs leading-5 text-slate-200">{JSON.stringify(value, null, 2)}</pre>;
}

function requiredString(form: FormData, name: string) {
  const value = optionalString(form, name);
  if (!value) throw new Error(`${name} is required`);
  return value;
}

function optionalString(form: FormData, name: string) {
  const value = form.get(name);
  return typeof value === "string" && value.trim() ? value.trim() : undefined;
}

function splitCsv(value: string | undefined) {
  return value ? value.split(",").map((part) => part.trim()).filter(Boolean) : [];
}

const demoSubgraphs: MemorySubgraph[] = [
  {
    id: "company_memory",
    tenant_id: "demo",
    project_id: "demo",
    parent_subgraph_id: null,
    name: "Company Memory",
    description: "Root governed graph",
    owner_kind: "team",
    owner_id: "platform",
    contract_id: "company-memory-v1",
    summary_claim_id: "claim_company_summary",
    permissions: { visibility: "internal" },
    status: "active"
  },
  {
    id: "engineering_memory",
    tenant_id: "demo",
    project_id: "demo",
    parent_subgraph_id: "company_memory",
    name: "Engineering",
    description: "Engineering bounded context",
    owner_kind: "team",
    owner_id: "engineering-platform",
    contract_id: "engineering-memory-v1",
    summary_claim_id: "claim_engineering_summary",
    permissions: { visibility: "restricted" },
    status: "active"
  },
  {
    id: "project_atlas",
    tenant_id: "demo",
    project_id: "demo",
    parent_subgraph_id: "engineering_memory",
    name: "Project Atlas",
    description: "Project memory",
    owner_kind: null,
    owner_id: null,
    contract_id: null,
    summary_claim_id: null,
    permissions: null,
    status: "draft"
  }
];

const demoEntities: CanonicalEntity[] = [
  {
    id: "canonical_customer_a",
    tenant_id: "demo",
    project_id: "demo",
    entity_type: "Customer",
    display_name: "Customer A",
    aliases: ["customer-a", "ACME Enterprise"]
  },
  {
    id: "canonical_project_atlas",
    tenant_id: "demo",
    project_id: "demo",
    entity_type: "Project",
    display_name: "Project Atlas",
    aliases: ["atlas-migration", "enterprise migration"]
  }
];
