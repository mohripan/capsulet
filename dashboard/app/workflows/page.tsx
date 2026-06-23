"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { Suspense, useCallback, useEffect, useMemo, useState } from "react";
import { FileCode2, GitBranch, LockKeyhole, Pencil, RefreshCw, Trash2, Workflow as WorkflowIcon } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { Workflow, deleteWorkflow, getErrorMessage, getWorkflowEditability, listWorkflows } from "../lib/api";

const workflowsPerPage = 8;

function graphLayers(workflow: Workflow) {
  const remaining = new Set(workflow.steps.map((step) => step.id));
  const placed = new Set<string>();
  const layers: typeof workflow.steps[] = [];
  while (remaining.size) {
    const layer = workflow.steps.filter((step) => remaining.has(step.id) && workflow.dependencies.filter((edge) => edge.to_step_id === step.id).every((edge) => placed.has(edge.from_step_id)));
    if (!layer.length) break;
    layers.push(layer);
    layer.forEach((step) => { remaining.delete(step.id); placed.add(step.id); });
  }
  return layers;
}

function WorkflowsContent() {
  const searchParams = useSearchParams();
  const requestedWorkflowId = searchParams.get("workflow");
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [catalogPage, setCatalogPage] = useState(0);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [selectedEditable, setSelectedEditable] = useState(true);

  const refresh = useCallback(async () => {
    setLoading(true); setError(null);
    try {
      const response = await listWorkflows();
      setWorkflows(response.workflows);
      if (requestedWorkflowId) {
        const requestedIndex = response.workflows.findIndex((workflow) => workflow.id === requestedWorkflowId);
        if (requestedIndex >= 0) setCatalogPage(Math.floor(requestedIndex / workflowsPerPage));
      }
      setSelectedId((current) => {
        const preferred = requestedWorkflowId || current;
        return response.workflows.some((workflow) => workflow.id === preferred) ? preferred : response.workflows[0]?.id ?? null;
      });
    } catch (err) { setError(getErrorMessage(err)); } finally { setLoading(false); }
  }, [requestedWorkflowId]);

  useEffect(() => { void refresh(); }, [refresh]);
  useEffect(() => {
    if (!selectedId) return;
    let active = true;
    void getWorkflowEditability(selectedId).then((result) => { if (active) setSelectedEditable(result.editable); }).catch(() => { if (active) setSelectedEditable(false); });
    return () => { active = false; };
  }, [selectedId]);
  const workflow = workflows.find((item) => item.id === selectedId);
  const layers = useMemo(() => workflow ? graphLayers(workflow) : [], [workflow]);
  const pageCount = Math.max(1, Math.ceil(workflows.length / workflowsPerPage));
  const visibleWorkflows = workflows.slice(catalogPage * workflowsPerPage, (catalogPage + 1) * workflowsPerPage);
  async function removeSelectedWorkflow() {
    if (!workflow || !window.confirm(`Delete workflow "${workflow.name}"? Automations and active runs must be removed first.`)) return;
    setError(null);
    try {
      await deleteWorkflow(workflow.id);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  return <DashboardShell actionLabel="New workflow" actionHref="/workflows/new">
    <PageHeader eyebrow="Workflow catalog" title="Workflows" description="Browse workflow definitions, inspect their dependency graph, and open a dedicated notebook when you need to author a new one." />
    <section className="workflowOverviewLayout">
      <section className="panel workflowCatalogPanel">
        <PanelTitle icon={WorkflowIcon} title="Definitions" action={`${workflows.length} total`} />
        <button className="secondaryButton workflowRefresh" onClick={refresh} disabled={loading}><RefreshCw size={16} />{loading ? "Refreshing" : "Refresh"}</button>
        {error ? <div className="errorBox" role="alert">{error}</div> : null}
        {!loading && !workflows.length ? <div className="emptyState">No workflows exist yet. Create a notebook to define the first one.</div> : null}
        <div className="workflowCatalog" role="list" aria-label="Workflow definitions">
          {visibleWorkflows.map((item) => <button className={item.id === selectedId ? "workflowCatalogItem active" : "workflowCatalogItem"} key={item.id} onClick={() => setSelectedId(item.id)} role="listitem">
            <span className="workflowCatalogTitle"><strong>{item.name}</strong><small>{item.status}</small></span>
            <span className="workflowCatalogMeta">{item.steps.length} cells · {item.dependencies.length} {item.dependencies.length === 1 ? "edge" : "edges"}</span>
            <span>{item.description || "No description"}</span>
          </button>)}
        </div>
        {workflows.length > workflowsPerPage ? <nav className="workflowPagination" aria-label="Workflow catalog pages"><button className="secondaryButton" disabled={catalogPage === 0} onClick={() => setCatalogPage((page) => page - 1)}>Previous</button><span>Page {catalogPage + 1} of {pageCount}</span><button className="secondaryButton" disabled={catalogPage + 1 >= pageCount} onClick={() => setCatalogPage((page) => page + 1)}>Next</button></nav> : null}
      </section>
      <section className="panel workflowTopologyPanel">
        <PanelTitle icon={GitBranch} title="Execution topology" action={workflow ? "Validated DAG" : "No selection"} />
        {!workflow ? <div className="emptyState">Select a workflow to inspect its execution stages.</div> : <>
          <header className="workflowTopologyHeader">
            <div><span>Selected workflow</span><h2>{workflow.name}</h2><p>{workflow.description || "No description"}</p></div>
            <div className="workflowTopologyActions">{selectedEditable ? <><Link className="secondaryButton" href={`/workflows/new?workflow=${encodeURIComponent(workflow.id)}`}><Pencil size={15} />Edit notebook</Link><button className="secondaryButton dangerButton" onClick={removeSelectedWorkflow} type="button"><Trash2 size={15} />Delete</button></> : <span className="secondaryButton disabledButton"><LockKeyhole size={15} />Notebook locked</span>}<dl><div><dt>Cells</dt><dd>{workflow.steps.length}</dd></div><div><dt>Edges</dt><dd>{workflow.dependencies.length}</dd></div><div><dt>Status</dt><dd>{workflow.status}</dd></div></dl></div>
          </header>
          <div className="dagCanvas workflowTopologyCanvas" aria-label={`${workflow.name} dependency graph`}>{layers.map((layer, layerIndex) => <div className="dagLayer" key={layerIndex}>
            <span className="dagLayerLabel">Stage {layerIndex + 1}</span>
            {layer.map((step) => <div className="dagNode" key={step.id}><FileCode2 size={17} /><div><strong>{step.name}</strong><small>{step.execution_pool}</small></div></div>)}
            {layerIndex < layers.length - 1 ? <GitBranch className="dagRail" aria-hidden="true" /> : null}
          </div>)}</div>
          <div className="workflowStepLedger">{workflow.steps.map((step) => <div key={step.id}><span>{String(step.position).padStart(2, "0")}</span><strong>{step.name}</strong><small>{step.execution_pool}</small></div>)}</div>
        </>}
      </section>
    </section>
  </DashboardShell>;
}

export default function WorkflowsPage() {
  return <Suspense fallback={<div className="emptyState">Loading workflows…</div>}><WorkflowsContent /></Suspense>;
}
