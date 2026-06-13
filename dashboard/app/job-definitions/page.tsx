"use client";

import Link from "next/link";
import { FormEvent, useEffect, useState } from "react";
import { FileCode2, Plus, RefreshCw, Send } from "lucide-react";
import { DashboardShell, PageHeader, PanelTitle } from "../components";
import {
  JobDefinition,
  createJobDefinition,
  getErrorMessage,
  listJobDefinitions
} from "../lib/api";

const starterScript = `from pathlib import Path

Path("/capsulet/artifacts").mkdir(parents=True, exist_ok=True)
Path("/capsulet/artifacts/report.txt").write_text("hello from a user-created job definition\\n")
print("job definition executed")
`;

export default function JobDefinitionsPage() {
  const [definitions, setDefinitions] = useState<JobDefinition[]>([]);
  const [name, setName] = useState("Daily report");
  const [runtimeImage, setRuntimeImage] = useState("python:3.12-slim");
  const [script, setScript] = useState(starterScript);
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
              <textarea value={script} onChange={(event) => setScript(event.target.value)} rows={12} />
            </label>
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
            {definitions.map((definition) => (
              <article className="resourceRow" key={definition.id}>
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
              </article>
            ))}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
