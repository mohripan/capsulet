"use client";

import { FormEvent, useCallback, useEffect, useMemo, useState } from "react";
import {
  Braces,
  CalendarClock,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Edit3,
  Eye,
  Play,
  PlugZap,
  Plus,
  Power,
  PowerOff,
  RefreshCw,
  Send,
  Trash2,
  Workflow
} from "lucide-react";
import Link from "next/link";
import { DashboardShell, DateTimePicker, DurationInput, PageHeader, PanelTitle, ResizableGridTable, StateBadge, defaultDateTimeRange } from "../components";
import {
  Automation,
  AutomationRequest,
  ContractField,
  JobDefinition,
  ParameterContract,
  TriggerCondition,
  TriggerKind,
  TriggerPlugin,
  Workflow as WorkflowDefinition,
  WorkflowRun,
  cancelWorkflowRun,
  capsuletStreamUrl,
  createAutomation,
  deleteAutomation,
  disableAutomation,
  enableAutomation,
  getErrorMessage,
  listAutomations,
  listJobDefinitions,
  listTriggerPlugins,
  listWorkflows,
  listWorkflowRuns,
  removeWorkflowRun,
  triggerAutomation,
  updateAutomation
} from "../lib/api";

type WizardStep = "details" | "triggers" | "condition";

type DraftTrigger = {
  name: string;
  kind: TriggerKind;
  pluginId: string;
  values: Record<string, unknown>;
};

const pageSize = 6;
const defaultRunRange = defaultDateTimeRange();

const workflowRunColumns = [
  { label: "Workflow run", width: 210, minWidth: 150, sortKey: "workflow_run" },
  { label: "Created", width: 165, minWidth: 140, sortKey: "created_at" },
  { label: "Workflow", width: 210, minWidth: 150, sortKey: "workflow" },
  { label: "Job runs", width: 165, minWidth: 130 },
  { label: "State", width: 120, minWidth: 100, sortKey: "state" },
  { label: "Actions", width: 160, minWidth: 145 },
  { label: "Automation", width: 190, minWidth: 140, sortKey: "automation" }
];
const workflowRunStates = ["queued", "running", "removed", "succeeded", "failed", "cancelled", "timed_out"];

const scheduleContract: ParameterContract = {
  fields: [
    { name: "cron", label: "Cron expression", type: "string", required: true, default: "0 0 * * * * *", placeholder: "0 */5 * * * * *" },
    { name: "timezone", label: "Timezone", type: "string", required: true, default: "UTC", placeholder: "Asia/Jakarta" }
  ]
};

const sqlContract: ParameterContract = {
  fields: [
    { name: "connection_name", label: "Connection", type: "string", required: true, placeholder: "inventory_readonly" },
    { name: "query", label: "Boolean query", type: "textarea", required: true, placeholder: "select exists(select 1 from inventory where quantity < 10)" },
    { name: "poll_seconds", label: "Poll every (seconds)", type: "number", required: true, default: 60 }
  ]
};

const webhookContract: ParameterContract = {
  fields: []
};

function defaultTrigger(): DraftTrigger {
  return {
    name: "schedule_ready",
    kind: "schedule",
    pluginId: "",
    values: defaultValues(scheduleContract)
  };
}

function fieldLabel(field: ContractField) {
  return field.label || field.name.replaceAll("_", " ");
}

function contractForTrigger(trigger: DraftTrigger, plugins: TriggerPlugin[]) {
  if (trigger.kind === "schedule") return scheduleContract;
  if (trigger.kind === "sql") return sqlContract;
  if (trigger.kind === "webhook") return webhookContract;
  if (trigger.kind === "custom") {
    return (plugins.find((plugin) => plugin.id === trigger.pluginId)?.config_schema || {}) as ParameterContract;
  }
  return { fields: [] };
}

function normalizeValue(field: ContractField, value: string | boolean) {
  if (field.type === "number") return Number(value);
  if (field.type === "boolean") return Boolean(value);
  return value;
}

function defaultValues(contract: ParameterContract) {
  return Object.fromEntries(
    (contract.fields || []).map((field) => [field.name, field.default ?? (field.type === "boolean" ? false : "")])
  );
}

function contractForWorkflow(workflow: WorkflowDefinition | undefined, definitions: JobDefinition[]): ParameterContract {
  if (!workflow) return { fields: [] };
  const definitionsById = new Map(definitions.map((definition) => [definition.id, definition]));
  const fields: ContractField[] = [];
  const seen = new Set<string>();
  for (const step of workflow.steps) {
    const definition = definitionsById.get(step.job_definition_id);
    for (const field of definition?.input_schema.fields ?? []) {
      if (seen.has(field.name)) continue;
      seen.add(field.name);
      fields.push({ ...field, label: `${fieldLabel(field)} · ${step.name}` });
    }
  }
  return { fields };
}

function conditionFromText(value: string): TriggerCondition {
  const tokens = value.trim().split(/\s+/).filter(Boolean);
  if (tokens.length === 1) return { trigger: tokens[0] };
  const op = tokens.find((token) => token.toUpperCase() === "OR") ? "any" : "all";
  const names = tokens.filter((token) => !["AND", "OR", "(", ")"].includes(token.toUpperCase()));
  const expressions = names.map((name) => ({ trigger: name }));
  return op === "any" ? { any: expressions } : { all: expressions };
}

function conditionToText(condition: TriggerCondition): string {
  if ("trigger" in condition) return condition.trigger;
  if ("all" in condition) return condition.all.map(conditionToText).join(" AND ");
  return condition.any.map(conditionToText).join(" OR ");
}

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value || "-";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short"
  }).format(date);
}

function isWorkflowRunTerminal(run: WorkflowRun) {
  return ["succeeded", "failed", "cancelled", "timed_out"].includes(run.status);
}

function latestStepRun(run: WorkflowRun) {
  return [...run.step_runs].sort((left, right) => right.position - left.position)[0];
}

export default function AutomationsPage() {
  const [automations, setAutomations] = useState<Automation[]>([]);
  const [jobDefinitions, setJobDefinitions] = useState<JobDefinition[]>([]);
  const [workflows, setWorkflows] = useState<WorkflowDefinition[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [tableWorkflowRuns, setTableWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [triggerPlugins, setTriggerPlugins] = useState<TriggerPlugin[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingAutomation, setEditingAutomation] = useState<Automation | null>(null);
  const [viewingAutomation, setViewingAutomation] = useState<Automation | null>(null);
  const [openAutomationMenuId, setOpenAutomationMenuId] = useState<string | null>(null);
  const [wizardStep, setWizardStep] = useState<WizardStep>("details");
  const [automationPage, setAutomationPage] = useState(1);
  const [runPage, setRunPage] = useState(1);
  const [runFilterStartAt, setRunFilterStartAt] = useState(defaultRunRange.start);
  const [runFilterEndAt, setRunFilterEndAt] = useState(defaultRunRange.end);
  const [runFilterText, setRunFilterText] = useState("");
  const [runFilterState, setRunFilterState] = useState("");
  const [runSortKey, setRunSortKey] = useState("created_at");
  const [runSortDirection, setRunSortDirection] = useState<"asc" | "desc">("desc");
  const [name, setName] = useState("Inventory email alert");
  const [automationStatus, setAutomationStatus] = useState<Automation["status"]>("enabled");
  const [workflowId, setWorkflowId] = useState("");
  const [jobInput, setJobInput] = useState<Record<string, unknown>>({});
  const [triggers, setTriggers] = useState<DraftTrigger[]>([defaultTrigger()]);
  const [condition, setCondition] = useState("schedule_ready");
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [lastLiveRefresh, setLastLiveRefresh] = useState<string | null>(null);
  const [liveStreamConnected, setLiveStreamConnected] = useState(false);
  const [isSubmitting, setIsSubmitting] = useState(false);

  const selectedWorkflow = useMemo(
    () => workflows.find((workflow) => workflow.id === workflowId),
    [workflows, workflowId]
  );
  const selectedWorkflowContract = useMemo(
    () => contractForWorkflow(selectedWorkflow, jobDefinitions),
    [jobDefinitions, selectedWorkflow]
  );

  const refresh = useCallback(async function refresh(silent = false) {
    if (!silent) setIsLoading(true);
    setError(null);
    try {
      const [automationResponse, jobResponse, workflowResponse, runResponse, tableRunResponse, pluginResponse] = await Promise.all([
        listAutomations(),
        listJobDefinitions(),
        listWorkflows(),
        listWorkflowRuns({ limit: 500 }),
        listWorkflowRuns({
          limit: 200,
          start_at: runFilterStartAt,
          end_at: runFilterEndAt,
          q: runFilterText,
          state: runFilterState,
          sort: runSortKey,
          direction: runSortDirection
        }),
        listTriggerPlugins()
      ]);
      setAutomations(automationResponse.automations);
      setJobDefinitions(jobResponse.job_definitions);
      setWorkflows(workflowResponse.workflows);
      setWorkflowRuns(runResponse.workflow_runs);
      setTableWorkflowRuns(tableRunResponse.workflow_runs);
      setTriggerPlugins(pluginResponse.trigger_plugins);
      setLastLiveRefresh(new Date().toISOString());
      setWorkflowId((current) => current || workflowResponse.workflows[0]?.id || "");
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      if (!silent) setIsLoading(false);
    }
  }, [runFilterEndAt, runFilterStartAt, runFilterState, runFilterText, runSortDirection, runSortKey]);

  useEffect(() => {
    setRunPage(1);
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let closed = false;
    const source = new EventSource(capsuletStreamUrl("/v1/events/stream"));
    source.addEventListener("open", () => {
      if (!closed) setLiveStreamConnected(true);
    });
    source.addEventListener("snapshot", () => {
      if (!closed) void refresh(true);
    });
    source.addEventListener("error", () => {
      if (!closed) setLiveStreamConnected(false);
    });
    return () => {
      closed = true;
      setLiveStreamConnected(false);
      source.close();
    };
  }, [refresh]);

  useEffect(() => {
    setJobInput((current) => ({ ...defaultValues(selectedWorkflowContract), ...current }));
  }, [selectedWorkflowContract]);

  function openCreateAutomation() {
    setEditingAutomation(null);
    setWizardStep("details");
    setName("Inventory email alert");
    setAutomationStatus("enabled");
    setWorkflowId((current) => current || workflows[0]?.id || "");
    setJobInput(defaultValues(selectedWorkflowContract));
    setTriggers([defaultTrigger()]);
    setCondition("schedule_ready");
    setIsModalOpen(true);
  }

  function openEditAutomation(automation: Automation) {
    setOpenAutomationMenuId(null);
    setEditingAutomation(automation);
    setWizardStep("details");
    setName(automation.name);
    setAutomationStatus(automation.status);
    setWorkflowId(automation.workflow_id);
    setJobInput(automation.job_input || {});
    setTriggers(
      automation.triggers.length
        ? automation.triggers.map((triggerItem) => ({
            name: triggerItem.name,
            kind: triggerItem.kind,
            pluginId: triggerItem.plugin_id || "",
            values: triggerItem.config
          }))
        : [defaultTrigger()]
    );
    setCondition(conditionToText(automation.condition));
    setIsModalOpen(true);
  }

  function closeModal() {
    setIsModalOpen(false);
    setEditingAutomation(null);
  }

  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setIsSubmitting(true);
    setError(null);
    setMessage(null);
    try {
      const request: AutomationRequest = {
        name,
        workflow_id: workflowId,
        status: automationStatus,
        trigger_kind: triggers.some((trigger) => trigger.kind === "schedule") ? "schedule" : "manual",
        job_input: jobInput,
        triggers: triggers.map((trigger) => ({
          name: trigger.name,
          kind: trigger.kind,
          config: trigger.values,
          plugin_id: trigger.kind === "custom" ? trigger.pluginId : undefined
        })),
        condition: conditionFromText(condition)
      };
      const automation = editingAutomation
        ? await updateAutomation(editingAutomation.id, request)
        : await createAutomation(request);
      setMessage(`${editingAutomation ? "Updated" : "Created"} ${automation.name}`);
      closeModal();
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSubmitting(false);
    }
  }

  async function trigger(id: string) {
    setOpenAutomationMenuId(null);
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

  async function toggleAutomation(automation: Automation) {
    setOpenAutomationMenuId(null);
    setError(null);
    setMessage(null);
    try {
      const updated =
        automation.status === "enabled"
          ? await disableAutomation(automation.id)
          : await enableAutomation(automation.id);
      setMessage(`${updated.status === "enabled" ? "Enabled" : "Disabled"} ${updated.name}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function removeAutomation(automation: Automation) {
    setOpenAutomationMenuId(null);
    if (!window.confirm(`Delete automation "${automation.name}"?`)) return;
    setError(null);
    setMessage(null);
    try {
      await deleteAutomation(automation.id);
      setMessage(`Deleted ${automation.name}`);
      setViewingAutomation((current) => (current?.id === automation.id ? null : current));
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function removeWorkflow(run: WorkflowRun) {
    if (!window.confirm(`Remove queued workflow run "${run.id}"?`)) return;
    setError(null);
    setMessage(null);
    try {
      const updated = await removeWorkflowRun(run.id);
      setMessage(`Removed workflow run ${updated.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  async function cancelWorkflow(run: WorkflowRun) {
    if (!window.confirm(`Cancel running workflow run "${run.id}"?`)) return;
    setError(null);
    setMessage(null);
    try {
      const updated = await cancelWorkflowRun(run.id);
      setMessage(`Cancelled workflow run ${updated.id}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    }
  }

  function updateTrigger(index: number, patch: Partial<DraftTrigger>) {
    setTriggers((current) =>
      current.map((trigger, itemIndex) => (itemIndex === index ? { ...trigger, ...patch } : trigger))
    );
  }

  function updateTriggerValue(index: number, field: ContractField, value: string | boolean) {
    setTriggers((current) =>
      current.map((trigger, itemIndex) =>
        itemIndex === index
          ? { ...trigger, values: { ...trigger.values, [field.name]: normalizeValue(field, value) } }
          : trigger
      )
    );
  }

  const pagedAutomations = automations.slice((automationPage - 1) * pageSize, automationPage * pageSize);
  const pagedRuns = tableWorkflowRuns.slice((runPage - 1) * pageSize, runPage * pageSize);
  const automationRunSummary = useMemo(() => {
    return new Map(
      automations.map((automation) => {
        const runs = workflowRuns
          .filter((run) => run.automation_id === automation.id)
          .sort((left, right) => new Date(right.created_at).getTime() - new Date(left.created_at).getTime());
        return [
          automation.id,
          {
            active: runs.find((run) => !isWorkflowRunTerminal(run)),
            latest: runs[0],
            total: runs.length
          }
        ];
      })
    );
  }, [automations, workflowRuns]);

  function handleWorkflowRunSort(nextSortKey: string) {
    if (nextSortKey === runSortKey) {
      setRunSortDirection((current) => (current === "asc" ? "desc" : "asc"));
      return;
    }
    setRunSortKey(nextSortKey);
    setRunSortDirection("asc");
  }

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Automation authoring"
        title="Attach trigger contracts to workflows"
        description="Create automations by choosing the workflow to run, filling its shared input parameters, and defining the condition that turns trigger signals into a workflow run."
      />

      <div className="pageToolbar">
        <button className="primaryAction" onClick={openCreateAutomation}>
          <Plus size={18} aria-hidden="true" />
          Automation
        </button>
        <button className="secondaryButton" onClick={() => void refresh()} disabled={isLoading}>
          <RefreshCw size={16} aria-hidden="true" />
          {isLoading ? "Refreshing" : "Refresh"}
        </button>
      </div>
      {error ? <div className="errorBox">{error}</div> : null}
      {message ? <div className="successBox">{message}</div> : null}

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Workflow} title="Automations" action="Live API" />
          <div className="liveMonitorBar">
            <span className="liveDot" aria-hidden="true" />
            <span>{lastLiveRefresh ? `Stream refresh ${formatDateTime(lastLiveRefresh)}` : "Stream starting"}</span>
            <span>{liveStreamConnected ? "SSE connected" : "SSE reconnecting"}</span>
          </div>
          <div className="resourceList">
            {!isLoading && automations.length === 0 ? <div className="emptyState">No automations yet.</div> : null}
            {pagedAutomations.map((automation) => {
              const runSummary = automationRunSummary.get(automation.id);
              const activeRun = runSummary?.active;
              const latestRun = runSummary?.latest;
              const latestJob = latestRun ? latestStepRun(latestRun) : undefined;
              return (
              <article className={`automationCard ${activeRun ? "hasActiveRun" : ""}`} key={automation.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    {automation.trigger_kind === "interval" ? <Clock3 size={19} /> : <Play size={19} />}
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p title={automation.workflow_id}>{automation.workflow_id}</p>
                    <div className="automationMetaLine">
                      <span>{automation.trigger_kind === "interval" ? "Scheduled" : "Manual"}</span>
                      <span>{automation.triggers.length} trigger{automation.triggers.length === 1 ? "" : "s"}</span>
                      {automation.interval_seconds ? <span>{automation.interval_seconds}s interval</span> : null}
                    </div>
                  </div>
                </div>
                <span className={`automationState ${automation.status}`}>{automation.status}</span>
                <div className="automationRunMonitor">
                  <div className="runMonitorHead">
                    <span className={activeRun ? "runPulse active" : "runPulse"} aria-hidden="true" />
                    <strong>{activeRun ? "Running now" : latestRun ? "Last job" : "No jobs yet"}</strong>
                    {latestRun ? <span className={`runStatusText state-${(activeRun?.status ?? latestRun.status).toLowerCase()}`}>{activeRun?.status ?? latestRun.status}</span> : null}
                  </div>
                  {latestRun ? (
                    <>
                      <div className="runMonitorMeta">
                        <span title={latestRun.created_at}>{formatDateTime(latestRun.created_at)}</span>
                        <span>{runSummary?.total ?? 0} run{runSummary?.total === 1 ? "" : "s"}</span>
                        <span>{latestRun.step_runs.length} job{latestRun.step_runs.length === 1 ? "" : "s"}</span>
                      </div>
                      <div className="runMonitorLink">
                        {latestJob?.job_run_id ? (
                          <Link href={`/runs/${latestJob.job_run_id}`} title={latestJob.job_run_id}>
                            Latest job {latestJob.position}: {latestJob.status}
                          </Link>
                        ) : latestJob ? (
                          <span>Latest step {latestJob.position}: {latestJob.status}</span>
                        ) : (
                          <span>No job run created yet</span>
                        )}
                      </div>
                    </>
                  ) : (
                    <div className="runMonitorMeta">
                      <span>Waiting for first trigger now</span>
                    </div>
                  )}
                </div>
                <div className="automationMenu">
                  <button
                    className="secondaryButton automationMenuButton"
                    type="button"
                    aria-expanded={openAutomationMenuId === automation.id}
                    onClick={() => setOpenAutomationMenuId((current) => (current === automation.id ? null : automation.id))}
                  >
                    Actions
                    <ChevronDown size={16} aria-hidden="true" />
                  </button>
                  {openAutomationMenuId === automation.id ? (
                    <div className="automationMenuList">
                      <button type="button" onClick={() => { setViewingAutomation(automation); setOpenAutomationMenuId(null); }}>
                        <Eye size={16} aria-hidden="true" />
                        View details
                      </button>
                      <button type="button" onClick={() => void trigger(automation.id)}>
                        <Play size={16} aria-hidden="true" />
                        Trigger
                      </button>
                      <button type="button" onClick={() => openEditAutomation(automation)}>
                        <Edit3 size={16} aria-hidden="true" />
                        Modify
                      </button>
                      <button type="button" onClick={() => void toggleAutomation(automation)}>
                        {automation.status === "enabled" ? <PowerOff size={16} aria-hidden="true" /> : <Power size={16} aria-hidden="true" />}
                        {automation.status === "enabled" ? "Disable" : "Enable"}
                      </button>
                      <button className="dangerMenuItem" type="button" onClick={() => void removeAutomation(automation)}>
                        <Trash2 size={16} aria-hidden="true" />
                        Delete
                      </button>
                    </div>
                  ) : null}
                </div>
              </article>
              );
            })}
          </div>
          <Pagination page={automationPage} total={automations.length} onPage={setAutomationPage} />
        </section>

        <section className="panel span4">
          <PanelTitle icon={PlugZap} title="Custom trigger plugins" action={`${triggerPlugins.length} registered`} />
          <div className="triggerPluginCallout">
            <p>
              Custom triggers are Python evaluators that run before a workflow. Author and test their script contract in a full-width workspace.
            </p>
            <Link className="primaryAction fullWidthAction" href="/trigger-plugins">
              <PlugZap size={16} aria-hidden="true" />
              Open trigger plugins
            </Link>
          </div>
        </section>

        <section className="panel span12">
          <PanelTitle icon={CalendarClock} title="Workflow Runs" action="Scheduler" />
          <div className="tableFilters">
            <label>
              <span>Start</span>
              <DateTimePicker value={runFilterStartAt} onChange={setRunFilterStartAt} />
            </label>
            <label>
              <span>End</span>
              <DateTimePicker value={runFilterEndAt} onChange={setRunFilterEndAt} />
            </label>
            <label>
              <span>Name</span>
              <input value={runFilterText} onChange={(event) => setRunFilterText(event.target.value)} placeholder="Workflow or automation" />
            </label>
            <label>
              <span>State</span>
              <select value={runFilterState} onChange={(event) => setRunFilterState(event.target.value)}>
                <option value="">All states</option>
                {workflowRunStates.map((state) => <option value={state} key={state}>{state}</option>)}
              </select>
            </label>
          </div>
          <ResizableGridTable columns={workflowRunColumns} sortKey={runSortKey} sortDirection={runSortDirection} onSort={handleWorkflowRunSort}>
            {pagedRuns.map((run) => (
              <div className="resizableRow runRow" key={run.id}>
                <span className="mono tableCell" title={run.id}>{run.id}</span>
                <span className="tableCell" title={run.created_at}>{formatDateTime(run.created_at)}</span>
                <span className="tableCell" title={run.workflow_id}>{run.workflow_id}</span>
                <span className="stepRunLinks">
                  {run.step_runs.length === 0 ? "-" : run.step_runs.map((stepRun) => (
                    stepRun.job_run_id ? (
                      <Link href={`/runs/${stepRun.job_run_id}`} key={stepRun.id}>{stepRun.position}: {stepRun.status}</Link>
                    ) : (
                      <span key={stepRun.id}>{stepRun.position}: {stepRun.status}</span>
                    )
                  ))}
                </span>
                <StateBadge state={run.status} />
                <span className="workflowRunActions">
                  <button
                    className="secondaryButton compactButton"
                    type="button"
                    disabled={run.status !== "queued" || run.step_runs.length > 0}
                    onClick={() => void removeWorkflow(run)}
                    title="Remove only before any node executes"
                  >
                    Remove
                  </button>
                  <button
                    className="secondaryButton compactButton dangerButton"
                    type="button"
                    disabled={run.status !== "running"}
                    onClick={() => void cancelWorkflow(run)}
                    title="Cancel a running workflow"
                  >
                    Cancel
                  </button>
                </span>
                <span className="tableCell" title={run.automation_id ?? ""}>{run.automation_id ?? "-"}</span>
              </div>
            ))}
          </ResizableGridTable>
          <Pagination page={runPage} total={tableWorkflowRuns.length} onPage={setRunPage} />
        </section>
      </section>

      {isModalOpen ? (
        <div className="modalBackdrop" role="dialog" aria-modal="true" aria-label={editingAutomation ? "Edit automation" : "Create automation"}>
          <form className="wizardModal" onSubmit={submit}>
            <div className="wizardHeader">
              <div><span>{editingAutomation ? "Edit automation" : "New automation"}</span><h2>{wizardStep === "details" ? "Details" : wizardStep === "triggers" ? "Triggers" : "Condition"}</h2></div>
              <button className="iconButton" type="button" onClick={closeModal}>x</button>
            </div>
            <div className="wizardSteps">
              {(["details", "triggers", "condition"] as WizardStep[]).map((step) => (
                <button type="button" className={wizardStep === step ? "active" : ""} onClick={() => setWizardStep(step)} key={step}>{step}</button>
              ))}
            </div>

            <div className="wizardBody">
              {wizardStep === "details" ? (
                <div className="formStack">
                  <label><span>Automation name</span><input value={name} onChange={(event) => setName(event.target.value)} /></label>
                  <label><span>Status</span><select value={automationStatus} onChange={(event) => setAutomationStatus(event.target.value as Automation["status"])}><option value="enabled">enabled</option><option value="disabled">disabled</option></select></label>
                  <label>
                    <span>Workflow</span>
                    <select value={workflowId} onChange={(event) => setWorkflowId(event.target.value)}>
                      <option value="">Choose workflow</option>
                      {workflows.map((workflow) => (
                        <option value={workflow.id} key={workflow.id}>
                          {workflow.name} · {workflow.steps.length} step{workflow.steps.length === 1 ? "" : "s"}
                        </option>
                      ))}
                    </select>
                  </label>
                  {selectedWorkflow ? (
                    <div className="workflowParameterSummary">
                      <strong>{selectedWorkflow.name}</strong>
                      <span>{selectedWorkflow.description || "No description"}</span>
                      <span>{selectedWorkflow.steps.length} job step{selectedWorkflow.steps.length === 1 ? "" : "s"} receive the same workflow input.</span>
                    </div>
                  ) : (
                    <div className="emptyState">Create a workflow before attaching automation triggers.</div>
                  )}
                  {selectedWorkflowContract.fields?.length ? (
                    <ContractFields contract={selectedWorkflowContract} values={jobInput} onChange={setJobInput} />
                  ) : selectedWorkflow ? (
                    <div className="conditionHelp"><Braces size={18} /><span>This workflow has no declared job parameters. The automation will trigger it with an empty input object.</span></div>
                  ) : null}
                </div>
              ) : null}

              {wizardStep === "triggers" ? (
                <div className="triggerBuilder">
                  {triggers.map((triggerItem, index) => {
                    const contract = contractForTrigger(triggerItem, triggerPlugins);
                    return (
                      <section className="triggerPanel" key={index}>
                        <div className="triggerPanelHead">
                          <strong>Trigger {index + 1}</strong>
                          <button type="button" className="secondaryButton" onClick={() => setTriggers((current) => current.filter((_, itemIndex) => itemIndex !== index))}>Remove</button>
                        </div>
                        <div className="formStack">
                          <label><span>Name</span><input value={triggerItem.name} onChange={(event) => updateTrigger(index, { name: event.target.value })} /></label>
                          <label><span>Kind</span><select value={triggerItem.kind} onChange={(event) => {
                            const kind = event.target.value as TriggerKind;
                            const nextContract = kind === "schedule" ? scheduleContract : kind === "sql" ? sqlContract : kind === "webhook" ? webhookContract : {};
                            updateTrigger(index, { kind, values: defaultValues(nextContract) });
                          }}><option value="manual">manual</option><option value="schedule">cron schedule</option><option value="sql">SQL condition</option><option value="webhook">signed webhook</option><option value="custom">custom plugin</option></select></label>
                          {triggerItem.kind === "custom" ? <label><span>Plugin</span><select value={triggerItem.pluginId} onChange={(event) => updateTrigger(index, { pluginId: event.target.value, values: defaultValues((triggerPlugins.find((plugin) => plugin.id === event.target.value)?.config_schema || {}) as ParameterContract) })}><option value="">Choose plugin</option>{triggerPlugins.map((plugin) => <option value={plugin.id} key={plugin.id}>{plugin.name}</option>)}</select></label> : null}
                          <ContractFields contract={contract} values={triggerItem.values} onChange={(values) => setTriggers((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, values } : item))} onFieldChange={(field, value) => updateTriggerValue(index, field, value)} />
                          {triggerItem.kind === "webhook" ? <div className="conditionHelp"><PlugZap size={18} /><span>Send signed JSON to /v1/webhooks/{editingAutomation?.id || "{automation_id}"}/{triggerItem.name || "{trigger_name}"}. Operators configure its HMAC secret.</span></div> : null}
                          {triggerItem.kind === "sql" ? <div className="conditionHelp"><Braces size={18} /><span>Connections are operator-managed. The query runs read-only with a five-second timeout and must return one boolean.</span></div> : null}
                          {triggerItem.kind === "custom" ? <div className="conditionHelp"><PlugZap size={18} /><span>The isolated plugin must print a final JSON line with matched and an optional object payload.</span></div> : null}
                        </div>
                      </section>
                    );
                  })}
                  <button type="button" className="secondaryButton" onClick={() => setTriggers((current) => [...current, defaultTrigger()])}><Plus size={16} />Trigger</button>
                </div>
              ) : null}

              {wizardStep === "condition" ? (
                <div className="formStack">
                  <div className="conditionHelp"><Braces size={18} /><span>Use trigger names with AND / OR. Example: schedule_ready AND inventory_low</span></div>
                  <label><span>Trigger condition</span><input value={condition} onChange={(event) => setCondition(event.target.value)} /></label>
                  <div className="triggerChips">{triggers.map((triggerItem) => <span key={triggerItem.name}>{triggerItem.name}</span>)}</div>
                </div>
              ) : null}
            </div>

            <div className="wizardFooter">
              <button type="button" className="secondaryButton" onClick={() => setWizardStep(wizardStep === "condition" ? "triggers" : "details")} disabled={wizardStep === "details"}><ChevronLeft size={16} />Back</button>
              {wizardStep !== "condition" ? (
                <button type="button" className="primaryAction" onClick={() => setWizardStep(wizardStep === "details" ? "triggers" : "condition")}>Next<ChevronRight size={16} /></button>
              ) : (
                <button className="primaryAction" disabled={isSubmitting || !workflowId}><Send size={16} />{isSubmitting ? "Saving" : editingAutomation ? "Save changes" : "Create automation"}</button>
              )}
            </div>
          </form>
        </div>
      ) : null}

      {viewingAutomation ? (
        <div className="modalBackdrop" role="dialog" aria-modal="true" aria-label="Automation details">
          <section className="wizardModal automationDetailModal">
            <div className="wizardHeader">
              <div>
                <span>Automation details</span>
                <h2>{viewingAutomation.name}</h2>
              </div>
              <button className="iconButton" type="button" onClick={() => setViewingAutomation(null)}>x</button>
            </div>
            <div className="automationDetailBody">
              <div className="detailGrid">
                <span>ID</span>
                <strong>{viewingAutomation.id}</strong>
                <span>Workflow</span>
                <strong>{viewingAutomation.workflow_id}</strong>
                <span>Status</span>
                <strong>{viewingAutomation.status}</strong>
                <span>Mode</span>
                <strong>{viewingAutomation.trigger_kind}</strong>
                <span>Interval</span>
                <strong>{viewingAutomation.interval_seconds ? `${viewingAutomation.interval_seconds}s` : "-"}</strong>
                <span>Condition</span>
                <strong>{conditionToText(viewingAutomation.condition)}</strong>
              </div>
              <section className="detailSection">
                <h3>Triggers</h3>
                <div className="automationTriggerDetails">
                  {viewingAutomation.triggers.length === 0 ? <div className="emptyState">No triggers configured.</div> : null}
                  {viewingAutomation.triggers.map((triggerItem) => (
                    <article className="automationTriggerDetail" key={triggerItem.name}>
                      <div>
                        <strong>{triggerItem.name}</strong>
                        <span>{triggerItem.kind}{triggerItem.plugin_id ? ` / ${triggerItem.plugin_id}` : ""}</span>
                      </div>
                      <span className={`automationState ${triggerItem.enabled ? "" : "disabled"}`}>{triggerItem.enabled ? "enabled" : "disabled"}</span>
                      <code>{JSON.stringify(triggerItem.config, null, 2)}</code>
                    </article>
                  ))}
                </div>
              </section>
              <section className="detailSection">
                <h3>Condition JSON</h3>
                <code className="detailCodeBlock">{JSON.stringify(viewingAutomation.condition, null, 2)}</code>
              </section>
              <section className="detailSection">
                <h3>Workflow input</h3>
                <code className="detailCodeBlock">{JSON.stringify(viewingAutomation.job_input || {}, null, 2)}</code>
              </section>
            </div>
          </section>
        </div>
      ) : null}
    </DashboardShell>
  );
}

function ContractFields({ contract, values, onChange, onFieldChange }: { contract: ParameterContract; values: Record<string, unknown>; onChange: (values: Record<string, unknown>) => void; onFieldChange?: (field: ContractField, value: string | boolean) => void }) {
  if (!contract.fields?.length) return null;
  return (
    <div className="contractFields">
      {contract.fields.map((field) => {
        const value = values[field.name] ?? field.default ?? "";
        const setValue = (next: string | boolean) => {
          if (onFieldChange) {
            onFieldChange(field, next);
          } else {
            onChange({ ...values, [field.name]: normalizeValue(field, next) });
          }
        };
        return (
          <label key={field.name}>
            <span>{fieldLabel(field)}{field.required ? " *" : ""}</span>
            {field.type === "textarea" ? (
              <textarea value={String(value)} placeholder={field.placeholder} onChange={(event) => setValue(event.target.value)} />
            ) : field.type === "boolean" ? (
              <input type="checkbox" checked={Boolean(value)} onChange={(event) => setValue(event.target.checked)} />
            ) : field.type === "datetime" ? (
              <DateTimePicker value={String(value)} onChange={setValue} />
            ) : field.name === "interval_seconds" && field.type === "number" ? (
              <DurationInput valueSeconds={Number(value) || 60} minSeconds={60} onChange={(seconds) => setValue(String(seconds))} />
            ) : (
              <input type={field.type === "password" ? "password" : field.type} value={String(value)} placeholder={field.placeholder} onChange={(event) => setValue(event.target.value)} />
            )}
          </label>
        );
      })}
    </div>
  );
}

function Pagination({ page, total, onPage }: { page: number; total: number; onPage: (page: number) => void }) {
  const pages = Math.max(1, Math.ceil(total / pageSize));
  return (
    <div className="pagination">
      <button className="secondaryButton" disabled={page <= 1} onClick={() => onPage(page - 1)}><ChevronLeft size={15} />Prev</button>
      <span>{page} / {pages}</span>
      <button className="secondaryButton" disabled={page >= pages} onClick={() => onPage(page + 1)}>Next<ChevronRight size={15} /></button>
    </div>
  );
}
