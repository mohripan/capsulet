"use client";

import { FormEvent, useEffect, useMemo, useState } from "react";
import {
  Braces,
  CalendarClock,
  ChevronLeft,
  ChevronRight,
  Clock3,
  Database,
  Edit3,
  Play,
  PlugZap,
  Plus,
  Power,
  PowerOff,
  RefreshCw,
  Save,
  Send,
  Trash2,
  Workflow
} from "lucide-react";
import Link from "next/link";
import { DashboardShell, PageHeader, PanelTitle, ResizableGridTable, StateBadge } from "../components";
import {
  Automation,
  AutomationRequest,
  ContractField,
  HostGroup,
  JobDefinition,
  ParameterContract,
  TriggerCondition,
  TriggerKind,
  TriggerPlugin,
  WorkflowRun,
  createAutomation,
  createTriggerPlugin,
  createWorkflow,
  deleteAutomation,
  disableAutomation,
  enableAutomation,
  getErrorMessage,
  listAutomations,
  listHostGroups,
  listJobDefinitions,
  listTriggerPlugins,
  listWorkflowRuns,
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

const workflowRunColumns = [
  { label: "Workflow run", width: 250, minWidth: 160 },
  { label: "Workflow", width: 250, minWidth: 160 },
  { label: "Job runs", width: 230, minWidth: 150 },
  { label: "State", width: 150, minWidth: 120 },
  { label: "Automation", width: 230, minWidth: 150 }
];

const scheduleContract: ParameterContract = {
  fields: [
    { name: "start_at", label: "Start at", type: "datetime", required: true },
    { name: "interval_seconds", label: "Repeat every seconds", type: "number", required: true, default: 3600 },
    { name: "window_seconds", label: "Valid window seconds", type: "number", required: true, default: 300 }
  ]
};

const sqlContract: ParameterContract = {
  fields: [
    { name: "connection_string", label: "Connection string", type: "password", required: true, placeholder: "postgres://user:pass@host:5432/db" },
    { name: "query", label: "Query", type: "textarea", required: true, placeholder: "select count(*) as value from inventory where quantity < 10" },
    { name: "true_when", label: "True when", type: "string", required: true, placeholder: "value > 0" }
  ]
};

function defaultTrigger(): DraftTrigger {
  return {
    name: "schedule_ready",
    kind: "schedule",
    pluginId: "",
    values: {
      start_at: new Date(Date.now() + 60_000).toISOString().slice(0, 16),
      interval_seconds: 3600,
      window_seconds: 300
    }
  };
}

function fieldLabel(field: ContractField) {
  return field.label || field.name.replaceAll("_", " ");
}

function contractForTrigger(trigger: DraftTrigger, plugins: TriggerPlugin[]) {
  if (trigger.kind === "schedule") return scheduleContract;
  if (trigger.kind === "sql") return sqlContract;
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

export default function AutomationsPage() {
  const [automations, setAutomations] = useState<Automation[]>([]);
  const [jobDefinitions, setJobDefinitions] = useState<JobDefinition[]>([]);
  const [hostGroups, setHostGroups] = useState<HostGroup[]>([]);
  const [workflowRuns, setWorkflowRuns] = useState<WorkflowRun[]>([]);
  const [triggerPlugins, setTriggerPlugins] = useState<TriggerPlugin[]>([]);
  const [isModalOpen, setIsModalOpen] = useState(false);
  const [editingAutomation, setEditingAutomation] = useState<Automation | null>(null);
  const [wizardStep, setWizardStep] = useState<WizardStep>("details");
  const [automationPage, setAutomationPage] = useState(1);
  const [runPage, setRunPage] = useState(1);
  const [name, setName] = useState("Inventory email alert");
  const [automationStatus, setAutomationStatus] = useState<Automation["status"]>("enabled");
  const [jobDefinitionId, setJobDefinitionId] = useState("");
  const [hostGroup, setHostGroup] = useState("");
  const [jobInput, setJobInput] = useState<Record<string, unknown>>({});
  const [jobInputText, setJobInputText] = useState("{}");
  const [triggers, setTriggers] = useState<DraftTrigger[]>([defaultTrigger()]);
  const [condition, setCondition] = useState("schedule_ready");
  const [pluginId, setPluginId] = useState("plugin_inventory_threshold");
  const [pluginName, setPluginName] = useState("Inventory threshold");
  const [pluginImage, setPluginImage] = useState("python:3.12-slim");
  const [pluginCommand, setPluginCommand] = useState("python,/plugin/check.py");
  const [pluginFields, setPluginFields] = useState<ContractField[]>([
    { name: "threshold", label: "Threshold", type: "number", required: true, default: 10 }
  ]);
  const [error, setError] = useState<string | null>(null);
  const [message, setMessage] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(true);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [isSavingPlugin, setIsSavingPlugin] = useState(false);

  const selectedJob = useMemo(
    () => jobDefinitions.find((definition) => definition.id === jobDefinitionId),
    [jobDefinitions, jobDefinitionId]
  );

  async function refresh() {
    setIsLoading(true);
    setError(null);
    try {
      const [automationResponse, jobResponse, hostResponse, runResponse, pluginResponse] = await Promise.all([
        listAutomations(),
        listJobDefinitions(),
        listHostGroups(),
        listWorkflowRuns(),
        listTriggerPlugins()
      ]);
      setAutomations(automationResponse.automations);
      setJobDefinitions(jobResponse.job_definitions);
      setHostGroups(hostResponse.host_groups);
      setWorkflowRuns(runResponse.workflow_runs);
      setTriggerPlugins(pluginResponse.trigger_plugins);
      setJobDefinitionId((current) => current || jobResponse.job_definitions[0]?.id || "");
      setHostGroup((current) => current || hostResponse.host_groups.find((item) => item.is_default)?.name || hostResponse.host_groups[0]?.name || "");
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
    if (!selectedJob) return;
    setJobInput((current) => ({ ...defaultValues(selectedJob.input_schema), ...current }));
  }, [selectedJob]);

  function openCreateAutomation() {
    setEditingAutomation(null);
    setWizardStep("details");
    setName("Inventory email alert");
    setAutomationStatus("enabled");
    setJobInput(selectedJob ? defaultValues(selectedJob.input_schema) : {});
    setJobInputText("{}");
    setTriggers([defaultTrigger()]);
    setCondition("schedule_ready");
    setIsModalOpen(true);
  }

  function openEditAutomation(automation: Automation) {
    setEditingAutomation(automation);
    setWizardStep("details");
    setName(automation.name);
    setAutomationStatus(automation.status);
    setJobInput(automation.job_input || {});
    setJobInputText(JSON.stringify(automation.job_input || {}, null, 2));
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
      const automationJobInput = editingAutomation ? JSON.parse(jobInputText) as Record<string, unknown> : jobInput;
      const workflowId = editingAutomation
        ? editingAutomation.workflow_id
        : (
            await createWorkflow({
              name: `${name} workflow`,
              description: `Generated by automation ${name}`,
              steps: [{ name: "Run selected job", job_definition_id: jobDefinitionId, execution_pool: hostGroup }]
            })
          ).id;
      const request: AutomationRequest = {
        name,
        workflow_id: workflowId,
        status: automationStatus,
        trigger_kind: triggers.some((trigger) => trigger.kind === "schedule") ? "schedule" : "manual",
        interval_seconds: Number(triggers.find((trigger) => trigger.kind === "schedule")?.values.interval_seconds) || undefined,
        job_input: automationJobInput,
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
        config_schema: { fields: pluginFields }
      });
      setMessage(`Saved trigger plugin ${plugin.name}`);
      await refresh();
    } catch (err) {
      setError(getErrorMessage(err));
    } finally {
      setIsSavingPlugin(false);
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

  async function toggleAutomation(automation: Automation) {
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
    if (!window.confirm(`Delete automation "${automation.name}"?`)) return;
    setError(null);
    setMessage(null);
    try {
      await deleteAutomation(automation.id);
      setMessage(`Deleted ${automation.name}`);
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
  const pagedRuns = workflowRuns.slice((runPage - 1) * pageSize, runPage * pageSize);

  return (
    <DashboardShell>
      <PageHeader
        eyebrow="Automation authoring"
        title="Attach trigger contracts to job workflows"
        description="Create automations by choosing the job, host group, trigger parameters, and the condition that turns trigger signals into a workflow run."
      />

      <div className="pageToolbar">
        <button className="primaryAction" onClick={openCreateAutomation}>
          <Plus size={18} aria-hidden="true" />
          Automation
        </button>
        <button className="secondaryButton" onClick={refresh} disabled={isLoading}>
          <RefreshCw size={16} aria-hidden="true" />
          {isLoading ? "Refreshing" : "Refresh"}
        </button>
      </div>
      {error ? <div className="errorBox">{error}</div> : null}
      {message ? <div className="successBox">{message}</div> : null}

      <section className="contentGrid">
        <section className="panel span8">
          <PanelTitle icon={Workflow} title="Automations" action="Live API" />
          <div className="resourceList">
            {!isLoading && automations.length === 0 ? <div className="emptyState">No automations yet.</div> : null}
            {pagedAutomations.map((automation) => (
              <article className="automationCard" key={automation.id}>
                <div className="resourceMain">
                  <div className="automationIcon">
                    {automation.trigger_kind === "interval" ? <Clock3 size={19} /> : <Play size={19} />}
                  </div>
                  <div>
                    <h2>{automation.name}</h2>
                    <p title={automation.workflow_id}>{automation.workflow_id}</p>
                  </div>
                </div>
                <span className={`automationState ${automation.status}`}>{automation.status}</span>
                <div className="triggerChips">
                  {automation.triggers.map((triggerItem) => (
                    <span key={triggerItem.name} title={triggerItem.name}>{triggerItem.name}</span>
                  ))}
                </div>
                <code className="conditionPreview" title={JSON.stringify(automation.condition)}>{JSON.stringify(automation.condition)}</code>
                <div className="automationActions">
                  <button className="secondaryButton" onClick={() => void trigger(automation.id)} disabled={automation.status !== "enabled"}>
                    <Play size={15} aria-hidden="true" />
                    Trigger
                  </button>
                  <button className="iconButton" type="button" title="Edit automation" onClick={() => openEditAutomation(automation)}>
                    <Edit3 size={16} aria-hidden="true" />
                  </button>
                  <button className="iconButton" type="button" title={automation.status === "enabled" ? "Disable automation" : "Enable automation"} onClick={() => void toggleAutomation(automation)}>
                    {automation.status === "enabled" ? <PowerOff size={16} aria-hidden="true" /> : <Power size={16} aria-hidden="true" />}
                  </button>
                  <button className="iconButton dangerButton" type="button" title="Delete automation" onClick={() => void removeAutomation(automation)}>
                    <Trash2 size={16} aria-hidden="true" />
                  </button>
                </div>
              </article>
            ))}
          </div>
          <Pagination page={automationPage} total={automations.length} onPage={setAutomationPage} />
        </section>

        <section className="panel span4">
          <PanelTitle icon={PlugZap} title="Custom Trigger Plugin" action="Registry" />
          <form className="formStack" onSubmit={savePlugin}>
            <label><span>Plugin id</span><input value={pluginId} onChange={(event) => setPluginId(event.target.value)} /></label>
            <label><span>Name</span><input value={pluginName} onChange={(event) => setPluginName(event.target.value)} /></label>
            <label><span>Runtime image</span><input value={pluginImage} onChange={(event) => setPluginImage(event.target.value)} /></label>
            <label><span>Command</span><input value={pluginCommand} onChange={(event) => setPluginCommand(event.target.value)} /></label>
            <div className="fieldContractList">
              {pluginFields.map((field, index) => (
                <div className="contractRow" key={`${field.name}-${index}`}>
                  <input value={field.name} onChange={(event) => setPluginFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, name: event.target.value } : item))} />
                  <select value={field.type} onChange={(event) => setPluginFields((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, type: event.target.value as ContractField["type"] } : item))}>
                    <option value="string">string</option>
                    <option value="number">number</option>
                    <option value="boolean">boolean</option>
                    <option value="textarea">textarea</option>
                    <option value="password">secret</option>
                  </select>
                </div>
              ))}
            </div>
            <button className="secondaryButton" type="button" onClick={() => setPluginFields((current) => [...current, { name: "value", type: "string", required: true }])}>
              <Plus size={15} aria-hidden="true" />
              Field
            </button>
            <button className="primaryAction fullWidthAction" disabled={isSavingPlugin}>
              <Save size={16} aria-hidden="true" />
              {isSavingPlugin ? "Saving" : "Save plugin"}
            </button>
          </form>
        </section>

        <section className="panel span12">
          <PanelTitle icon={CalendarClock} title="Workflow Runs" action="Scheduler" />
          <ResizableGridTable columns={workflowRunColumns}>
            {pagedRuns.map((run) => (
              <div className="resizableRow runRow" key={run.id}>
                <span className="mono tableCell" title={run.id}>{run.id}</span>
                <span className="tableCell" title={run.workflow_id}>{run.workflow_id}</span>
                <span className="stepRunLinks">
                  {run.step_runs.length === 0 ? "-" : run.step_runs.map((stepRun) => (
                    <Link href={`/runs/${stepRun.job_run_id}`} key={stepRun.id}>{stepRun.position}: {stepRun.status}</Link>
                  ))}
                </span>
                <StateBadge state={run.status} />
                <span className="tableCell" title={run.automation_id ?? ""}>{run.automation_id ?? "-"}</span>
              </div>
            ))}
          </ResizableGridTable>
          <Pagination page={runPage} total={workflowRuns.length} onPage={setRunPage} />
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
                  {editingAutomation ? (
                    <div className="readonlyField">
                      <span>Workflow</span>
                      <code title={editingAutomation.workflow_id}>{editingAutomation.workflow_id}</code>
                    </div>
                  ) : (
                    <>
                      <label><span>Job definition</span><select value={jobDefinitionId} onChange={(event) => setJobDefinitionId(event.target.value)}>{jobDefinitions.map((definition) => <option value={definition.id} key={definition.id}>{definition.name}</option>)}</select></label>
                      <label><span>Host group</span><select value={hostGroup} onChange={(event) => setHostGroup(event.target.value)}>{hostGroups.map((group) => <option value={group.name} key={group.name}>{group.name}</option>)}</select></label>
                    </>
                  )}
                  {editingAutomation ? (
                    <label>
                      <span>Job parameters JSON</span>
                      <textarea value={jobInputText} onChange={(event) => setJobInputText(event.target.value)} />
                    </label>
                  ) : (
                    <ContractFields contract={selectedJob?.input_schema || {}} values={jobInput} onChange={setJobInput} />
                  )}
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
                            updateTrigger(index, { kind, values: defaultValues(kind === "schedule" ? scheduleContract : kind === "sql" ? sqlContract : {}) });
                          }}><option value="manual">manual</option><option value="schedule">schedule</option><option value="sql">sql</option><option value="custom">custom plugin</option></select></label>
                          {triggerItem.kind === "custom" ? <label><span>Plugin</span><select value={triggerItem.pluginId} onChange={(event) => updateTrigger(index, { pluginId: event.target.value, values: defaultValues((triggerPlugins.find((plugin) => plugin.id === event.target.value)?.config_schema || {}) as ParameterContract) })}><option value="">Choose plugin</option>{triggerPlugins.map((plugin) => <option value={plugin.id} key={plugin.id}>{plugin.name}</option>)}</select></label> : null}
                          <ContractFields contract={contract} values={triggerItem.values} onChange={(values) => setTriggers((current) => current.map((item, itemIndex) => itemIndex === index ? { ...item, values } : item))} onFieldChange={(field, value) => updateTriggerValue(index, field, value)} />
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
                <button className="primaryAction" disabled={isSubmitting || (!editingAutomation && (!jobDefinitionId || !hostGroup))}><Send size={16} />{isSubmitting ? "Saving" : editingAutomation ? "Save changes" : "Create automation"}</button>
              )}
            </div>
          </form>
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
            {field.type === "textarea" ? <textarea value={String(value)} placeholder={field.placeholder} onChange={(event) => setValue(event.target.value)} /> : field.type === "boolean" ? <input type="checkbox" checked={Boolean(value)} onChange={(event) => setValue(event.target.checked)} /> : <input type={field.type === "datetime" ? "datetime-local" : field.type === "password" ? "password" : field.type} value={String(value)} placeholder={field.placeholder} onChange={(event) => setValue(event.target.value)} />}
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
