"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import { FileCode2, GitBranch, Plus, RefreshCw, Send, Trash2, Workflow as WorkflowIcon } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import { ExecutionPool, JobDefinition, Workflow, createWorkflow, getErrorMessage, listExecutionPools, listJobDefinitions, listWorkflows } from "../lib/api";

type StepDraft = { key: string; name: string; jobDefinitionId: string; dependsOn: string[] };

const initialSteps: StepDraft[] = [
  { key: "source-a", name: "Extract customers", jobDefinitionId: "", dependsOn: [] },
  { key: "source-b", name: "Extract orders", jobDefinitionId: "", dependsOn: [] },
  { key: "merge", name: "Merge reports", jobDefinitionId: "", dependsOn: ["source-a", "source-b"] }
];

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

export default function WorkflowsPage() {
  const [workflows, setWorkflows] = useState<Workflow[]>([]);
  const [selected, setSelected] = useState(0);
  const [definitions, setDefinitions] = useState<JobDefinition[]>([]);
  const [pools, setPools] = useState<ExecutionPool[]>([]);
  const [name, setName] = useState("Daily reporting DAG");
  const [description, setDescription] = useState("Extract in parallel, then merge the results.");
  const [steps, setSteps] = useState<StepDraft[]>(initialSteps);
  const [pool, setPool] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [submitting, setSubmitting] = useState(false);

  async function refresh() {
    setLoading(true); setError(null);
    try {
      const [workflowResponse, definitionResponse, poolResponse] = await Promise.all([listWorkflows(), listJobDefinitions(), listExecutionPools()]);
      setWorkflows(workflowResponse.workflows);
      setDefinitions(definitionResponse.job_definitions);
      setPools(poolResponse.execution_pools);
      const first = definitionResponse.job_definitions[0]?.id ?? "";
      setSteps((current) => current.map((step, index) => ({ ...step, jobDefinitionId: step.jobDefinitionId || definitionResponse.job_definitions[index]?.id || first })));
      setPool((current) => current || poolResponse.execution_pools.find((item) => item.is_default)?.name || poolResponse.execution_pools[0]?.name || "");
    } catch (err) { setError(getErrorMessage(err)); } finally { setLoading(false); }
  }

  useEffect(() => { void refresh(); }, []);
  useEffect(() => { if (selected >= workflows.length) setSelected(Math.max(0, workflows.length - 1)); }, [selected, workflows.length]);

  function updateStep(key: string, update: Partial<StepDraft>) {
    setSteps((current) => current.map((step) => step.key === key ? { ...step, ...update } : step));
  }

  function addStep() {
    const key = `step-${crypto.randomUUID()}`;
    setSteps((current) => [...current, { key, name: `Step ${current.length + 1}`, jobDefinitionId: definitions[0]?.id ?? "", dependsOn: [] }]);
  }

  function removeStep(key: string) {
    setSteps((current) => current.filter((step) => step.key !== key).map((step) => ({ ...step, dependsOn: step.dependsOn.filter((parent) => parent !== key) })));
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setSubmitting(true); setError(null);
    const prefix = `step_${Date.now()}`;
    const ids = new Map(steps.map((step, index) => [step.key, `${prefix}_${index + 1}`]));
    try {
      await createWorkflow({ name, description, steps: steps.map((step) => ({ id: ids.get(step.key), name: step.name, job_definition_id: step.jobDefinitionId, execution_pool: pool })), dependencies: steps.flatMap((step) => step.dependsOn.map((parent) => ({ from_step_id: ids.get(parent)!, to_step_id: ids.get(step.key)! }))) });
      await refresh();
    } catch (err) { setError(getErrorMessage(err)); } finally { setSubmitting(false); }
  }

  const workflow = workflows[selected];
  const layers = useMemo(() => workflow ? graphLayers(workflow) : [], [workflow]);

  return <DashboardShell>
    <PageHeader eyebrow="Workflow authoring" title="Build dependency graphs" description="Define independent roots, fan-out work, and fan-in gates. Capsulet validates every graph before it can run." />
    <section className="contentGrid">
      <section className="panel span5">
        <PanelTitle icon={Plus} title="New workflow" action="DAG editor" />
        <form className="formStack" onSubmit={submit}>
          <label><span>Name</span><input aria-label="Workflow name" value={name} onChange={(event) => setName(event.target.value)} required /></label>
          <label><span>Description</span><input value={description} onChange={(event) => setDescription(event.target.value)} /></label>
          <label><span>Execution pool</span><select value={pool} onChange={(event) => setPool(event.target.value)}>{pools.map((item) => <option value={item.name} key={item.name}>{item.name}</option>)}</select></label>
          <div className="dagStepEditor">
            {steps.map((step, index) => <fieldset className="dagStep" key={step.key}>
              <legend>Node {index + 1}</legend>
              <label><span>Node name</span><input value={step.name} onChange={(event) => updateStep(step.key, { name: event.target.value })} required /></label>
              <label><span>Job definition</span><select value={step.jobDefinitionId} onChange={(event) => updateStep(step.key, { jobDefinitionId: event.target.value })}>{definitions.map((definition) => <option value={definition.id} key={definition.id}>{definition.name}</option>)}</select></label>
              <div className="dependencyPicker"><span>Depends on</span>{index === 0 ? <small>Root node — starts immediately</small> : steps.slice(0, index).map((candidate) => <label key={candidate.key}><input type="checkbox" checked={step.dependsOn.includes(candidate.key)} onChange={(event) => updateStep(step.key, { dependsOn: event.target.checked ? [...step.dependsOn, candidate.key] : step.dependsOn.filter((key) => key !== candidate.key) })} />{candidate.name}</label>)}</div>
              {steps.length > 1 ? <button type="button" className="textButton dangerButton" onClick={() => removeStep(step.key)}><Trash2 size={14} />Remove node</button> : null}
            </fieldset>)}
          </div>
          <button type="button" className="secondaryButton" onClick={addStep}><Plus size={15} />Add node</button>
          <button className="primaryAction fullWidthAction" disabled={submitting || !pool || steps.some((step) => !step.jobDefinitionId)}><Send size={16} />{submitting ? "Creating" : "Create workflow"}</button>
          {error ? <div className="errorBox" role="alert">{error}</div> : null}
        </form>
      </section>
      <section className="panel span7">
        <PanelTitle icon={WorkflowIcon} title="Execution topology" action="Validated DAG" />
        <div className="panelActions"><button className="secondaryButton" onClick={refresh} disabled={loading}><RefreshCw size={16} />{loading ? "Refreshing" : "Refresh"}</button></div>
        {!loading && !workflow ? <div className="emptyState">No graph exists yet. Add at least one node and create a workflow.</div> : null}
        {workflow ? <article className="workflowCard dagCard">
          <div className="poolTop"><div><h2>{workflow.name}</h2><p>{workflow.id} / {workflow.status}</p></div><span>{workflow.steps.length} nodes · {workflow.dependencies.length} edges</span></div>
          <div className="dagCanvas" aria-label={`${workflow.name} dependency graph`}>{layers.map((layer, layerIndex) => <div className="dagLayer" key={layerIndex}>
            <span className="dagLayerLabel">Stage {layerIndex + 1}</span>
            {layer.map((step) => { const parents = workflow.dependencies.filter((edge) => edge.to_step_id === step.id).length; const children = workflow.dependencies.filter((edge) => edge.from_step_id === step.id).length; return <div className="dagNode" key={step.id}><FileCode2 size={17} /><div><strong>{step.name}</strong><small>{parents === 0 ? "Root" : `${parents} prerequisite${parents === 1 ? "" : "s"}`} · {children} downstream</small></div></div>; })}
            {layerIndex < layers.length - 1 ? <GitBranch className="dagRail" aria-hidden="true" /> : null}
          </div>)}</div>
          <div className="workflowPager"><button className="secondaryButton" disabled={selected === 0} onClick={() => setSelected((value) => value - 1)}>Previous</button><span>{selected + 1} / {workflows.length}</span><button className="secondaryButton" disabled={selected >= workflows.length - 1} onClick={() => setSelected((value) => value + 1)}>Next</button></div>
        </article> : null}
      </section>
    </section>
  </DashboardShell>;
}
