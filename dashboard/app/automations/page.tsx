"use client";

import { FormEvent, useEffect, useState } from "react";
import { Clock3, Database, Play, PlugZap, Plus, RefreshCw, Send, Workflow } from "lucide-react";
import Link from "next/link";
import { DashboardShell, PageHeader, PanelTitle, StateBadge } from "../components";
import {
  Automation,
  TriggerKind,
  TriggerPlugin,
  Workflow as WorkflowDefinition,
  WorkflowRun,
  createAutomation,
  createTriggerPlugin,
  getErrorMessage,
  listAutomations,
  listTriggerPlugins,
  listWorkflowRuns,
  listWorkflows,
  triggerAutomation
} from "../lib/api";

export default function AutomationsPage() {
  const [automations, setAutomations] = useState<Automation[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowDefinition[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [triggerPlugins, setTriggerPlugins] = useState<TriggerPlugin[]>([]);
  const [name, setName] = useState("Nightly customer pipeline");
  const [workflowId, setWorkflowId] = useState("");
  const [triggerKind, setTriggerKind] = useState<TriggerKind>("schedule");
  const [intervalSeconds, setIntervalSeconds] = useState(3600);
  const [sqlConnectionName, setSqlConnectionName] = useState("orders");
  const [sqlQuery, setSqlQuery] = useState("select count(*) from orders where status = 'pending'");
  const [customPluginId, setCustomPluginId] = useState("");
  const [customConfig, setCustomConfig] = useState("{\n  \"threshold\": 10\n}");
  const [includeSqlGate, setIncludeSqlGate] = useState(false);
  const [pluginId, setPluginId] = useState("plugin_customer_threshold");
  const [pluginName, setPluginName] = useState("Customer threshold");
  const [pluginImage, setPluginImage] = useState("python:3.12-slim");
  const [pluginCommand, setPluginCommand] = useState("python,/plugin/check.py");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isSavingPlugin, setIsSavingPlugin] = useState(false);

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const [automationResponse, workflowResponse, runResponse, pluginResponse] = await Promise.all([
        listAutomations(),
        listWorkflows(),
        listWorkflowRuns(),
        listTriggerPlugins()
      ]);
      setAutomations(automationResponse.automations);
      setWorkflows(workflowResponse.workflows);
      setWorkflowRuns(runResponse.workflow_runs);
      setTriggerPlugins(pluginResponse.trigger_plugins);
      setWorkflowId((current) => current || workflowResponse.workflows[0]?.id || "");
      setCustomPluginId((current) => current || pluginResponse.trigger_plugins[0]?.id || "");
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
    setMessage(null);
    try {
      const primaryTriggerName = triggerKind === "custom" ? "custom_plugin" : triggerKind;
      const triggers = [
        {
          name: primaryTriggerName,
          kind: triggerKind,
          config: triggerConfig(triggerKind),
          plugin_id: triggerKind === "custom" ? customPluginId : undefined
        }
      ];
      if (includeSqlGate && triggerKind !== "sql") {
        triggers.push({
          name: "sql_gate",
          kind: "sql",
          config: {
            connection_name: sqlConnectionName,
            query: sqlQuery
          },
          plugin_id: undefined
        });
      }
      const automation = await createAutomation({
        name,
        workflow_id: workflowId,
        trigger_kind: triggerKind === "schedule" ? "schedule" : "manual",
        interval_seconds: triggerKind === "schedule" ? intervalSeconds : undefined,
        triggers,
        condition:
          includeSqlGate && triggerKind !== "sql"
            ? { all: [{ trigger: primaryTriggerName }, { trigger: "sql_gate" }] }
            : { trigger: primaryTriggerName }
      });
      setMessage(`Created ${automation.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  async function savePlugin(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSavingPlugin(true);
    setError(null);
    setMessage(null);
    try {
      const plugin = await createTriggerPlugin({
        id: pluginId,
        name: pluginName,
        runtime_image: pluginImage,
        command: pluginCommand.split(",").map((part) => part.trim()).filter(Boolean),
        config_schema: { type: "object" }
      });
      setMessage(`Saved trigger plugin ${plugin.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSavingPlugin(false);
    }
  }

  function triggerConfig(kind: TriggerKind) {
    if (kind === "schedule") {
      return { interval_seconds: intervalSeconds };
    }
    if (kind === "sql") {
      return { connection_name: sqlConnectionName, query: sqlQuery };
    }
    if (kind === "custom") {
      return parseJsonObject(customConfig);
    }
    return {};
  }

  function parseJsonObject(value: string) {
    try {
      const parsed = JSON.parse(value) as unknown;
      return parsed && typeof parsed === "object" && !Array.isArray(parsed) ? (parsed as Record<string, unknown>) : {};
    } catch {
      return {};
    }
  }

  async function trigger(id: string) {
    setError(null);
    setMessage(null);
    try {
      const run = await triggerAutomation(id);
      setMessage(`Triggered workflow run ${run.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Trigger graph authoring"
        title="Build automations from schedule, SQL, and plugin triggers"
        description="Bind workflow jobs to named trigger definitions, then compose those triggers into the condition that creates workflow runs."
      />

      <section className="contentGrid">
        <section className="panel span5">
          <PanelTitle icon={Plus} title="New Automation" action="Trigger graph" />
          <form className="formStack" onSubmit={submit}>
            <label>
              <span>Name</span>
              <input value={name} onChange={(event) => setName(event.target.value)} />
            </label>
            <label>
              <span>Workflow</span>
              <select value={workflowId} onChange={(event) => setWorkflowId(event.target.value)}>
                {workflows.map((workflow) => (
                  <option value={workflow.id} key={workflow.id}>
                    {workflow.name}
                  </option>
                ))}
              </select>
            </label>
            <label>
              <span>Primary trigger</span>
              <select value={triggerKind} onChange={(event) => setTriggerKind(event.target.value as TriggerKind)}>
                <option value="manual">manual</option>
                <option value="schedule">schedule</option>
                <option value="sql">sql</option>
                <option value="custom">custom plugin</option>
              </select>
            </label>
            {triggerKind === "schedule" ? (
              <label>
                <span>Interval seconds</span>
                <input
                  type="number"
                  min="30"
                  value={intervalSeconds}
                  onChange={(event) => setIntervalSeconds(Number(event.target.value))}
                />
              </label>
            ) : null}
            {triggerKind === "sql" || includeSqlGate ? (
              <>
                <label>
                  <span>SQL connection</span>
                  <input value={sqlConnectionName} onChange={(event) => setSqlConnectionName(event.target.value)} />
                </label>
                <label>
                  <span>SQL trigger query</span>
                  <textarea value={sqlQuery} onChange={(event) => setSqlQuery(event.target.value)} />
                </label>
              </>
            ) : null}
            {triggerKind === "custom" ? (
              <>
                <label>
                  <span>Plugin</span>
                  <select value={customPluginId} onChange={(event) => setCustomPluginId(event.target.value)}>
                    {triggerPlugins.map((plugin) => (
                      <option value={plugin.id} key={plugin.id}>
                        {plugin.name}
                      </option>
                    ))}
                  </select>
                </label>
                <label>
                  <span>Plugin config JSON</span>
                  <textarea value={customConfig} onChange={(event) => setCustomConfig(event.target.value)} />
                </label>
              </>
            ) : null}
            {triggerKind !== "sql" ? (
              <label className="checkItem compactCheck">
                <input type="checkbox" checked={includeSqlGate} onChange={(event) => setIncludeSqlGate(event.target.checked)} />
                <span>Require SQL gate trigger</span>
              </label>
            ) : null}
            <button className="primaryAction inlineAction" disabled={isSubmitting || !workflowId}>
              <Send size={16} aria-hidden="true" />
              {isSubmitting ? "Creating" : "Create automation"}
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={Workflow} title="Automations" action="Live API" />
          <div className="panelActions">
            <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
              <RefreshCw size={16} aria-hidden="true" />
              {isLoading ? "Refreshing" : "Refresh"}
            </button>
          </div>
          {error ? <div className="errorBox">{error}</div> : null}
          {message ? <div className="successBox">{message}</div> : null}
          <div className="resourceList">
            {!isLoading && automations.length === 0 ? (
              <div className="emptyState">No automations yet. Create a workflow, then bind it here.</div>
            ) : null}
            {automations.map((automation) => (
              <article className="resourceRow" key={automation.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    {automation.trigger_kind === "interval" ? <Clock3 size={19} /> : <Play size={19} />}
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p>{automation.workflow_id}</p>
                  </div>
                </div>
                <span className="tableCell">{automation.trigger_kind}</span>
                <span className="tableCell">
                  {automation.triggers.map((triggerItem) => triggerItem.name).join(" + ") || "-"}
                </span>
                <button className="secondaryButton" onClick={() => void trigger(automation.id)}>
                  <Play size={15} aria-hidden="true" />
                  Trigger
                </button>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span5">
          <PanelTitle icon={PlugZap} title="Custom Trigger Plugin" action="Registry" />
          <form className="formStack" onSubmit={savePlugin}>
            <label>
              <span>Plugin id</span>
              <input value={pluginId} onChange={(event) => setPluginId(event.target.value)} />
            </label>
            <label>
              <span>Name</span>
              <input value={pluginName} onChange={(event) => setPluginName(event.target.value)} />
            </label>
            <label>
              <span>Runtime image</span>
              <input value={pluginImage} onChange={(event) => setPluginImage(event.target.value)} />
            </label>
            <label>
              <span>Command</span>
              <input value={pluginCommand} onChange={(event) => setPluginCommand(event.target.value)} />
            </label>
            <button className="primaryAction inlineAction" disabled={isSavingPlugin}>
              <PlugZap size={16} aria-hidden="true" />
              {isSavingPlugin ? "Saving" : "Save plugin"}
            </button>
          </form>
        </section>

        <section className="panel span7">
          <PanelTitle icon={Database} title="Trigger Plugins" action="Live API" />
          <div className="resourceList">
            {triggerPlugins.length === 0 ? (
              <div className="emptyState">No custom trigger plugins registered.</div>
            ) : null}
            {triggerPlugins.map((plugin) => (
              <article className="pluginRow" key={plugin.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    <PlugZap size={18} aria-hidden="true" />
                  </div>
                  <div>
                    <h2>{plugin.name}</h2>
                    <p>{plugin.id}</p>
                  </div>
                </div>
                <span className="tableCell">{plugin.runtime_image}</span>
                <span className="tableCell">{plugin.command.join(" ")}</span>
              </article>
            ))}
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={Clock3} title="Workflow Runs" action="Scheduler" />
          <div className="runTable">
            <div className="runHeader liveRunGrid">
              <span>Workflow run</span>
              <span>Workflow</span>
              <span>Job runs</span>
              <span>State</span>
              <span>Automation</span>
            </div>
            {workflowRuns.map((run) => (
              <div className="runRow liveRunGrid" key={run.id}>
                <span className="mono tableCell" title={run.id}>{run.id}</span>
                <span className="tableCell" title={run.workflow_id}>{run.workflow_id}</span>
                <span className="stepRunLinks">
                  {run.step_runs.length === 0 ? (
                    "-"
                  ) : (
                    run.step_runs.map((stepRun) => (
                      <Link href={`/runs/${stepRun.job_run_id}`} key={stepRun.id} title={stepRun.job_run_id}>
                        {stepRun.position}: {stepRun.status}
                      </Link>
                    ))
                  )}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell" title={run.automation_id ?? ""}>{run.automation_id ?? "-"}</span>
              </div>
            ))}
          </div>
        </section>
      </section>
    </DashboardShell>
  );
}
