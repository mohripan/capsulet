"use client";

import Link from "next/link";
import { useSearchParams } from "next/navigation";
import { FormEvent, Suspense, useEffect, useState } from "react";
import { ArrowLeft, Braces, GripVertical, LockKeyhole, Plus, Save, Send, Trash2 } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, PythonEditor } from "../../components";
import {
  ExecutionPool,
  JobDefinition,
  createJobDefinition,
  createWorkflow,
  getErrorMessage,
  getJobDefinitionSource,
  getWorkflow,
  getWorkflowEditability,
  listExecutionPools,
  listJobDefinitions,
  updateJobDefinition,
  updateWorkflow
} from "../../lib/api";

type CellDraft = {
  key: string;
  stepId?: string;
  jobDefinitionId?: string;
  originalDefinition?: {
    name: string;
    code: string;
    runtimeImage: string;
    pythonDependencies: string;
  };
  name: string;
  code: string;
  runtimeImage: string;
  pythonDependencies: string;
  pool: string;
  outputs: string;
  dependsOn: string[];
};

const exampleCells: CellDraft[] = [
  { key: "generate-csv", name: "Generate customers CSV", runtimeImage: "python:3.12-slim", pythonDependencies: "", pool: "", outputs: "customers.csv", dependsOn: [], code: `import csv
from pathlib import Path

output = Path("/capsulet/artifacts/customers.csv")
output.parent.mkdir(parents=True, exist_ok=True)
with output.open("w", newline="") as handle:
    writer = csv.DictWriter(handle, fieldnames=["customer", "orders", "revenue"])
    writer.writeheader()
    writer.writerows([
        {"customer": "Ada", "orders": 3, "revenue": 420},
        {"customer": "Grace", "orders": 5, "revenue": 810},
        {"customer": "Linus", "orders": 2, "revenue": 190},
    ])
print(f"Wrote {output}")` },
  { key: "summarize-csv", name: "Summarize customer revenue", runtimeImage: "python:3.12-slim", pythonDependencies: "", pool: "", outputs: "customer-summary.csv", dependsOn: ["generate-csv"], code: `import csv
from pathlib import Path

source = Path("/capsulet/inputs/customers.csv")
output = Path("/capsulet/artifacts/customer-summary.csv")
with source.open(newline="") as handle:
    rows = list(csv.DictReader(handle))
rows.sort(key=lambda row: int(row["revenue"]), reverse=True)
with output.open("w", newline="") as handle:
    writer = csv.DictWriter(handle, fieldnames=["customer", "revenue"])
    writer.writeheader()
    writer.writerows({"customer": row["customer"], "revenue": row["revenue"]} for row in rows)
print(f"Wrote {output}")` }
];

function slug(value: string) {
  return value.toLowerCase().trim().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "") || "cell";
}

function dependencyLines(value: string) {
  return value.split(/\r?\n/).map((line) => line.trim()).filter(Boolean);
}

function WorkflowNotebookContent() {
  const searchParams = useSearchParams();
  const workflowId = searchParams.get("workflow");
  const editing = Boolean(workflowId);
  const [pools, setPools] = useState<ExecutionPool[]>([]);
  const [jobDefinitions, setJobDefinitions] = useState<JobDefinition[]>([]);
  const [name, setName] = useState("Customer revenue notebook");
  const [description, setDescription] = useState("Generate a CSV, then transform it in a dependent Python cell.");
  const [cells, setCells] = useState<CellDraft[]>(exampleCells);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [loading, setLoading] = useState(editing);
  const [submitting, setSubmitting] = useState(false);
  const [lockReason, setLockReason] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    void Promise.all([
      listExecutionPools(),
      listJobDefinitions(),
      workflowId ? getWorkflow(workflowId) : Promise.resolve(null),
      workflowId ? getWorkflowEditability(workflowId) : Promise.resolve({ editable: true, reason: null })
    ]).then(async ([poolResponse, definitionResponse, workflow, editability]) => {
      if (cancelled) return;
      setPools(poolResponse.execution_pools);
      setJobDefinitions(definitionResponse.job_definitions);
      setLockReason(editability.editable ? null : editability.reason || "This workflow has an active execution.");
      const defaultPool = poolResponse.execution_pools.find((item) => item.is_default)?.name || poolResponse.execution_pools[0]?.name || "";
      if (!workflow) {
        setCells((current) => current.map((cell) => ({ ...cell, pool: cell.pool || defaultPool })));
        return;
      }
      const definitions = new Map(definitionResponse.job_definitions.map((definition) => [definition.id, definition]));
      const sources = await Promise.all(workflow.steps.map((step) => getJobDefinitionSource(step.job_definition_id)));
      if (cancelled) return;
      setName(workflow.name);
      setDescription(workflow.description);
      setCells(workflow.steps.map((step, index) => ({
        key: step.id,
        stepId: step.id,
        jobDefinitionId: step.job_definition_id,
        originalDefinition: {
          name: step.name,
          code: sources[index].python_script,
          runtimeImage: definitions.get(step.job_definition_id)?.runtime_image || "python:3.12-slim",
          pythonDependencies: sources[index].python_dependencies.join("\n")
        },
        name: step.name,
        code: sources[index].python_script,
        runtimeImage: definitions.get(step.job_definition_id)?.runtime_image || "python:3.12-slim",
        pythonDependencies: sources[index].python_dependencies.join("\n"),
        pool: step.execution_pool || defaultPool,
        outputs: "",
        dependsOn: workflow.dependencies.filter((edge) => edge.to_step_id === step.id).map((edge) => edge.from_step_id)
      })));
    }).catch((err) => setError(getErrorMessage(err))).finally(() => {
      if (!cancelled) setLoading(false);
    });
    return () => { cancelled = true; };
  }, [workflowId]);

  function updateCell(key: string, update: Partial<CellDraft>) {
    setCells((current) => current.map((cell) => cell.key === key ? { ...cell, ...update } : cell));
  }

  async function selectJobDefinition(cellKey: string, definitionId: string) {
    if (!definitionId) {
      updateCell(cellKey, { jobDefinitionId: undefined, originalDefinition: undefined });
      return;
    }
    const definition = jobDefinitions.find((item) => item.id === definitionId);
    if (!definition) return;
    setError(null);
    try {
      const source = await getJobDefinitionSource(definition.id);
      updateCell(cellKey, {
        jobDefinitionId: definition.id,
        originalDefinition: {
          name: definition.name,
          code: source.python_script,
          runtimeImage: definition.runtime_image,
          pythonDependencies: source.python_dependencies.join("\n")
        },
        name: definition.name,
        code: source.python_script,
        runtimeImage: definition.runtime_image,
        pythonDependencies: source.python_dependencies.join("\n")
      });
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  function addCell() {
    const key = `cell-${crypto.randomUUID()}`;
    setCells((current) => [...current, { key, name: `Python cell ${current.length + 1}`, code: "from pathlib import Path\n\noutput = Path(\"/capsulet/artifacts/result.txt\")\noutput.write_text(\"hello from Capsulet\\n\")", runtimeImage: "python:3.12-slim", pythonDependencies: "", pool: current[0]?.pool || pools[0]?.name || "", outputs: "result.txt", dependsOn: current.length ? [current[current.length - 1].key] : [] }]);
  }

  function removeCell(key: string) {
    setCells((current) => current.filter((cell) => cell.key !== key).map((cell) => ({ ...cell, dependsOn: cell.dependsOn.filter((parent) => parent !== key) })));
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault(); setSubmitting(true); setError(null); setMessage(null);
    const workflowKey = workflowId || `${slug(name)}-${Date.now()}`;
    const stepIds = new Map(cells.map((cell, index) => [cell.key, cell.stepId || `${workflowKey}-${slug(cell.name)}-${index + 1}`]));
    try {
      const jobDefinitionIds: string[] = [];
      for (const cell of cells) {
        const request = { name: cell.name, runtime_image: cell.runtimeImage, python_script: cell.code, python_dependencies: dependencyLines(cell.pythonDependencies), retry_max_attempts: 1, retry_delay_seconds: 0 };
        const unchangedReusableDefinition = cell.jobDefinitionId && cell.originalDefinition
          && cell.name === cell.originalDefinition.name
          && cell.runtimeImage === cell.originalDefinition.runtimeImage
          && cell.pythonDependencies === cell.originalDefinition.pythonDependencies
          && cell.code === cell.originalDefinition.code;
        if (cell.jobDefinitionId && unchangedReusableDefinition) {
          jobDefinitionIds.push(cell.jobDefinitionId);
        } else if (cell.jobDefinitionId) {
          const definition = await updateJobDefinition(cell.jobDefinitionId, request);
          jobDefinitionIds.push(definition.id);
        } else {
          const definition = await createJobDefinition({ ...request, id: `job-${stepIds.get(cell.key)}` });
          jobDefinitionIds.push(definition.id);
        }
      }
      const request = {
        name,
        description,
        steps: cells.map((cell, index) => ({ id: stepIds.get(cell.key), name: cell.name, job_definition_id: jobDefinitionIds[index], execution_pool: cell.pool })),
        dependencies: cells.flatMap((cell) => cell.dependsOn.map((parent) => ({ from_step_id: stepIds.get(parent)!, to_step_id: stepIds.get(cell.key)! })))
      };
      if (workflowId) await updateWorkflow(workflowId, request);
      else await createWorkflow(request);
      setMessage(`${workflowId ? "Updated" : "Created"} ${name} with ${cells.length} Python cells.`);
    } catch (err) { setError(getErrorMessage(err)); } finally { setSubmitting(false); }
  }

  return <DashboardShell>
    <div className="notebookBack"><Link className="textButton" href="/workflows"><ArrowLeft size={15} />Back to workflows</Link></div>
    <PageHeader eyebrow="Notebook authoring" title={editing ? "Edit workflow" : "Create workflow"} description="Write each unit of work where it runs. Dependencies become graph edges; artifacts become inputs for downstream cells." />
    <section className="panel notebookPanel notebookPanelFull">
      <PanelTitle icon={Braces} title="Workflow notebook" action={loading ? "Loading" : `${cells.length} cells`} />
      {loading ? <div className="emptyState">Loading notebook source…</div> : <form className="formStack" onSubmit={submit}>
        {lockReason ? <div className="workflowLockNotice" role="status"><LockKeyhole size={18} /><div><strong>Notebook locked</strong><span>{lockReason} Wait for those executions to finish or cancel them before editing.</span></div></div> : null}
        <fieldset className="notebookFieldset" disabled={Boolean(lockReason)}>
        <div className="notebookMeta"><label><span>Workflow name</span><input aria-label="Workflow name" value={name} onChange={(event) => setName(event.target.value)} required /></label><label><span>Description</span><input value={description} onChange={(event) => setDescription(event.target.value)} /></label></div>
        <div className="notebookRail">{cells.map((cell, index) => <article className="notebookCell" key={cell.key}>
          <div className="cellGutter" aria-hidden="true"><span>{String(index + 1).padStart(2, "0")}</span><GripVertical size={16} /></div>
          <div className="cellBody"><div className="cellHeader"><label><span>Cell name</span><input aria-label={`Cell ${index + 1} name`} value={cell.name} onChange={(event) => updateCell(cell.key, { name: event.target.value })} required /></label>{cells.length > 1 ? <button type="button" className="iconButton dangerButton" aria-label={`Remove ${cell.name}`} onClick={() => removeCell(cell.key)}><Trash2 size={15} /></button> : null}</div>
            <label className="cellDefinitionPicker">
              <span>Reusable script</span>
              <select value={cell.jobDefinitionId ?? ""} onChange={(event) => void selectJobDefinition(cell.key, event.target.value)}>
                <option value="">Custom Python cell — create/update from this workflow</option>
                {jobDefinitions.map((definition) => (
                  <option value={definition.id} key={definition.id}>
                    {definition.name} · {definition.runtime_image}
                  </option>
                ))}
              </select>
            </label>
            <PythonEditor value={cell.code} onChange={(code) => updateCell(cell.key, { code })} rows={index === 0 ? 15 : 17} />
            <div className="cellSettings"><label><span>Runtime image</span><input value={cell.runtimeImage} onChange={(event) => updateCell(cell.key, { runtimeImage: event.target.value })} required /></label><label><span>Execution pool</span><select value={cell.pool} onChange={(event) => updateCell(cell.key, { pool: event.target.value })}>{pools.map((pool) => <option value={pool.name} key={pool.name}>{pool.name}</option>)}</select></label><label><span>Artifact files</span><input value={cell.outputs} onChange={(event) => updateCell(cell.key, { outputs: event.target.value })} placeholder="report.csv" /></label></div>
            <label className="cellPackageEditor"><span>Python packages</span><textarea value={cell.pythonDependencies} onChange={(event) => updateCell(cell.key, { pythonDependencies: event.target.value })} rows={3} placeholder={"requests==2.32.5\npandas>=2.2,<3"} /></label>
            <div className="cellDependencies"><span>Runs after</span>{index === 0 ? <small>Starts immediately</small> : cells.slice(0, index).map((candidate) => <label key={candidate.key}><input type="checkbox" checked={cell.dependsOn.includes(candidate.key)} onChange={(event) => updateCell(cell.key, { dependsOn: event.target.checked ? [...cell.dependsOn, candidate.key] : cell.dependsOn.filter((key) => key !== candidate.key) })} />{candidate.name}</label>)}</div>
          </div></article>)}</div>
        <div className="notebookActions"><button type="button" className="secondaryButton" onClick={addCell}><Plus size={15} />Add Python cell</button><button className="primaryAction" disabled={submitting || cells.some((cell) => !cell.pool || !cell.code.trim())}>{editing ? <Save size={16} /> : <Send size={16} />}{submitting ? "Saving workflow" : editing ? "Save changes" : "Create workflow"}</button></div>
        </fieldset>
        {message ? <div className="successBox notebookSuccess" role="status">{message} <Link href={`/workflows${workflowId ? `?workflow=${encodeURIComponent(workflowId)}` : ""}`}>View workflow</Link></div> : null}{error ? <div className="errorBox" role="alert">{error}</div> : null}
      </form>}
    </section>
  </DashboardShell>;
}

export default function NewWorkflowPage() {
  return <Suspense fallback={<div className="emptyState">Loading notebook…</div>}><WorkflowNotebookContent /></Suspense>;
}
