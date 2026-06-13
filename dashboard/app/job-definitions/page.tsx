"use client";

import Link from "next/link";
import { FormEvent, useEffect, useState } from "react";
import { FileCode2, Plus, RefreshCw, Send, Trash2 } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle, PythonEditor } from "../components";
import {
  ContractField,
  JobDefinition,
  createJobDefinition,
  getErrorMessage,
  listJobDefinitions
} from "../lib/api";

const starterScript = `from pathlib import Path
import json
import os

params = json.loads(os.environ.get("CAPSULET_INPUT_JSON", "{}"))

Path("/capsulet/artifacts").mkdir(parents=True, exist_ok=True)
Path("/capsulet/artifacts/report.txt").write_text(f"hello {params.get('recipient', 'operator')}\\n")
print("job definition executed", params)
`;

const pageSize = 6;

export default function JobDefinitionsPage() {
  const [definitions, setDefinitions] = useState<JobDefinition[]>([]);
  const [definitionPage, setDefinitionPage] = useState(1);
  const [name, setName] = useState("Daily report");
  const [runtimeImage, setRuntimeImage] = useState("python:3.12-slim");
  const [script, setScript] = useState(starterScript);
  const [contractFields, setContractFields] = useState<ContractField[]>([
    { name: "recipient", label: "Recipient", type: "string", required: true, default: "mohripan16@gmail.com" },
    { name: "subject", label: "Subject", type: "string", required: true, default: "Capsulet test" },
    { name: "body", label: "Body", type: "textarea", required: true, default: "Hello from Capsulet" }
  ]);
  const [error, setError] = useState<string | null>(null);
  const [created, setCreated] = useState<JobDefinition | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const response = await listJobDefinitions();
      setDefinitions(response.job_definitions);
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsLoading(false);
    }
  }

  useEffect(() => {
    void refresh();
  }, []);

  useEffect(() => {
    const pages = Math.max(1, Math.ceil(definitions.length / pageSize));
    if (definitionPage > pages) {
      setDefinitionPage(pages);
    }
  }, [definitionPage, definitions.length]);

  const pagedDefinitions = definitions.slice((definitionPage - 1) * pageSize, definitionPage * pageSize);

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);
    setCreated(null);
    try {
      const definition = await createJobDefinition({
        name,
        runtime_image: runtimeImage,
        python_script: script,
        input_schema: { fields: contractFields },
        retry_max_attempts: 1,
        retry_delay_seconds: 0
      });
      setCreated(definition);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Authoring"
        title="Create reusable Python job definitions"
        description="Define scripts once, then submit them directly or compose them into workflows in the next MVP slice."
      />

      <section className="contentGrid">
        <section className="panel span5">
          <PanelTitle icon={Plus} title="New Python Job" action="Live API" />
          <form className="formStack" onSubmit={submit}>
            <label>
              <span>Name</span>
              <input value={name} onChange={(event) => setName(event.target.value)} />
            </label>
            <label>
              <span>Runtime image</span>
              <input value={runtimeImage} onChange={(event) => setRuntimeImage(event.target.value)} />
            </label>
            <label>
              <span>Python script</span>
              <PythonEditor value={script} onChange={setScript} />
            </label>
            <div className="contractEditor">
              <div className="sectionTitle">Job parameters</div>
              {contractFields.map((field, index) => (
                <div className="contractRow" key={`${field.name}-${index}`}>
                  <input value={field.name} onChange={(event) => setContractFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} />
                  <select value={field.type} onChange={(event) => setContractFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, type: event.target.value as ContractField["type"] } : item))}>
                    <option value="string">string</option>
                    <option value="number">number</option>
                    <option value="boolean">boolean</option>
                    <option value="textarea">textarea</option>
                    <option value="password">secret</option>
                  </select>
                  <button className="iconButton" type="button" onClick={() => setContractFields((current) => current.filter((_, itemIndex) => itemIndex !== index))}>
                    <Trash2 size={15} aria-hidden="true" />
                  </button>
                </div>
              ))}
              <button className="secondaryButton" type="button" onClick={() => setContractFields((current) => [...current, { name: "value", type: "string", required: true }])}>
                <Plus size={15} aria-hidden="true" />
                Parameter
              </button>
            </div>
            <button className="primaryAction inlineAction" disabled={isSubmitting}>
              <Send size={16} aria-hidden="true" />
              {isSubmitting ? "Creating" : "Create job definition"}
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={FileCode2} title="Job Definitions" action="Reusable scripts" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {created ? (
            <div className="successBox">
              Created {created.id}. Submit it from <Link href="/runs">Runs</Link>.
            </div>
          ) : null}
          <div className="resourceList">
            {!isLoading && definitions.length === 0 ? (
              <div className="emptyState">No job definitions yet. Create a Python job definition to reuse it.</div>
            ) : null}
            {pagedDefinitions.map((definition) => (
              <article className="resourceRow jobDefinitionRow" key={definition.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    <FileCode2 size={19} aria-hidden="true" />
                  </div>
                  <div>
                    <h2 title={definition.name}>{definition.name}</h2>
                    <p title={definition.id}>{definition.id}</p>
                  </div>
                </div>
                <span className="tableCell" title={definition.runtime_image}>
                  {definition.runtime_image}
                </span>
                <span className="tableCell" title={definition.bundle_object_key}>
                  {definition.bundle_object_key}
                </span>
                <span className="tableCell">
                  {definition.input_schema.fields?.length ?? 0} params
                </span>
              </article>
            ))}
          </div>
          <Pagination page={definitionPage} total={definitions.length} onPage={setDefinitionPage} />
        </section>
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
