export type RunStatus =
  | "queued"
  | "leased"
  | "running"
  | "removed"
  | "succeeded"
  | "failed"
  | "cancelled"
  | "timed_out"
  | "retry_scheduled";

export type JobRun = {
  id: string;
  job_definition_id: string;
  status: RunStatus;
  execution_pool: string;
  host_group: string;
  attempt_count: number;
  created_at: string;
  input: Record<string, unknown>;
};

export type LogsResponse = {
  run_id: string;
  logs: string;
  object_log_available: boolean;
};

export type WorkflowRunLogsResponse = {
  workflow_run_id: string;
  workflow_id: string;
  status: WorkflowRun["status"];
  entries: WorkflowRunLogEntry[];
};

export type WorkflowRunLogEntry = {
  step_run_id: string;
  workflow_step_id: string;
  job_run_id: string;
  position: number;
  status: WorkflowRun["status"];
  logs: string;
  object_log_available: boolean;
};

export type Artifact = {
  id: string;
  name: string;
  object_key: string;
  content_type: string;
  size_bytes: number;
  checksum_sha256: string | null;
  kind: "bundle" | "log" | "artifact";
};

export type SubmitRunRequest = {
  job_definition_id: string;
  execution_pool: string;
  host_group?: string;
  run_id?: string;
  python_script?: string;
  input?: Record<string, unknown>;
};

export type JobDefinition = {
  id: string;
  name: string;
  runtime_image: string;
  command: string[];
  bundle_object_key: string;
  input_schema: ParameterContract;
  retry_max_attempts: number;
  retry_delay_seconds: number;
};

export type ContractField = {
  name: string;
  label?: string;
  type: "string" | "number" | "boolean" | "datetime" | "textarea" | "password";
  required?: boolean;
  default?: string | number | boolean;
  placeholder?: string;
};

export type ParameterContract = {
  fields?: ContractField[];
};

export type CreateJobDefinitionRequest = {
  id?: string;
  name: string;
  runtime_image?: string;
  python_script: string;
  input_schema?: ParameterContract;
  retry_max_attempts?: number;
  retry_delay_seconds?: number;
};

export type ExecutionPool = {
  name: string;
  description: string;
  is_default: boolean;
  host_group: string;
};

export type HostGroup = {
  name: string;
  description: string;
  is_default: boolean;
  execution_pool: string;
  host_count: number | null;
};

export type WorkflowStep = {
  id: string;
  position: number;
  name: string;
  job_definition_id: string;
  execution_pool: string;
  host_group: string;
};

export type Workflow = {
  id: string;
  name: string;
  description: string;
  status: "draft" | "enabled" | "disabled";
  steps: WorkflowStep[];
};

export type CreateWorkflowRequest = {
  name: string;
  description?: string;
  steps: Array<{
    name: string;
    job_definition_id: string;
    execution_pool: string;
  }>;
};

export type Automation = {
  id: string;
  name: string;
  description: string;
  workflow_id: string;
  status: "enabled" | "disabled";
  trigger_kind: "manual" | "interval";
  interval_seconds: number | null;
  triggers: AutomationTrigger[];
  condition: TriggerCondition;
  job_input: Record<string, unknown>;
};

export type TriggerKind = "manual" | "schedule" | "sql" | "custom";

export type TriggerCondition =
  | { trigger: string }
  | { all: TriggerCondition[] }
  | { any: TriggerCondition[] };

export type AutomationTrigger = {
  name: string;
  kind: TriggerKind;
  config: Record<string, unknown>;
  plugin_id: string | null;
  enabled: boolean;
};

export type AutomationRequest = {
  name: string;
  description?: string;
  workflow_id: string;
  status?: "enabled" | "disabled";
  trigger_kind?: "manual" | "interval" | "schedule";
  interval_seconds?: number;
  job_input?: Record<string, unknown>;
  triggers?: Array<{
    name: string;
    kind: TriggerKind;
    config: Record<string, unknown>;
    plugin_id?: string;
    enabled?: boolean;
  }>;
  condition?: TriggerCondition;
};

export type TriggerPlugin = {
  id: string;
  name: string;
  description: string;
  runtime_image: string;
  command: string[];
  config_schema: Record<string, unknown>;
};

export type WorkflowRun = {
  id: string;
  workflow_id: string;
  automation_id: string | null;
  status: "queued" | "running" | "removed" | "succeeded" | "failed" | "cancelled" | "timed_out";
  current_step_position: number;
  created_at: string;
  step_runs: WorkflowStepRun[];
};

export type WorkflowStepRun = {
  id: string;
  workflow_step_id: string;
  job_run_id: string;
  position: number;
  status: "queued" | "running" | "removed" | "succeeded" | "failed" | "cancelled" | "timed_out";
};

export class CapsuletApiError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code?: string
  ) {
    super(message);
    this.name = "CapsuletApiError";
  }
}

export type TableQuery = {
  limit?: number;
  start_at?: string;
  end_at?: string;
  q?: string;
  state?: string;
  sort?: string;
  direction?: "asc" | "desc";
};

function queryString(query: TableQuery) {
  const params = new URLSearchParams();
  for (const [key, value] of Object.entries(query)) {
    if (value !== undefined && value !== "") {
      params.set(key, String(value));
    }
  }
  const value = params.toString();
  return value ? `?${value}` : "";
}

async function apiFetch<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(`/api/capsulet${path}`, {
    ...init,
    cache: "no-store",
    headers: {
      "content-type": "application/json",
      ...(init?.headers ?? {})
    }
  });

  if (!response.ok) {
    let message = `API returned ${response.status}`;
    let code: string | undefined;
    try {
      const body = (await response.json()) as { code?: string; message?: string };
      message = body.message || message;
      code = body.code;
    } catch {
      message = response.statusText || message;
    }
    throw new CapsuletApiError(message, response.status, code);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

export function getErrorMessage(error: unknown) {
  if (error instanceof CapsuletApiError) {
    return error.code ? `${error.code}: ${error.message}` : error.message;
  }
  if (error instanceof Error) {
    return error.message;
  }
  return "Unexpected dashboard error";
}

export async function listRuns(query: number | TableQuery = 50) {
  const params = typeof query === "number" ? { limit: query } : query;
  return apiFetch<{ runs: JobRun[] }>(`/v1/jobs/runs${queryString(params)}`);
}

export async function listJobDefinitions(limit = 100) {
  return apiFetch<{ job_definitions: JobDefinition[] }>(`/v1/job-definitions?limit=${limit}`);
}

export async function createJobDefinition(request: CreateJobDefinitionRequest) {
  return apiFetch<JobDefinition>("/v1/job-definitions", {
    method: "POST",
    body: JSON.stringify(request)
  });
}

export async function listExecutionPools() {
  return apiFetch<{ execution_pools: ExecutionPool[] }>("/v1/execution-pools");
}

export async function listHostGroups() {
  return apiFetch<{ host_groups: HostGroup[] }>("/v1/host-groups");
}

export async function listWorkflows() {
  return apiFetch<{ workflows: Workflow[] }>("/v1/workflows");
}

export async function createWorkflow(request: CreateWorkflowRequest) {
  return apiFetch<Workflow>("/v1/workflows", {
    method: "POST",
    body: JSON.stringify(request)
  });
}

export async function listAutomations() {
  return apiFetch<{ automations: Automation[] }>("/v1/automations");
}

export async function createAutomation(request: AutomationRequest) {
  return apiFetch<Automation>("/v1/automations", {
    method: "POST",
    body: JSON.stringify(request)
  });
}

export async function updateAutomation(id: string, request: AutomationRequest) {
  return apiFetch<Automation>(`/v1/automations/${encodeURIComponent(id)}`, {
    method: "PUT",
    body: JSON.stringify(request)
  });
}

export async function deleteAutomation(id: string) {
  return apiFetch<void>(`/v1/automations/${encodeURIComponent(id)}`, {
    method: "DELETE"
  });
}

export async function enableAutomation(id: string) {
  return apiFetch<Automation>(`/v1/automations/${encodeURIComponent(id)}/enable`, {
    method: "POST",
    body: "{}"
  });
}

export async function disableAutomation(id: string) {
  return apiFetch<Automation>(`/v1/automations/${encodeURIComponent(id)}/disable`, {
    method: "POST",
    body: "{}"
  });
}

export async function listTriggerPlugins() {
  return apiFetch<{ trigger_plugins: TriggerPlugin[] }>("/v1/trigger-plugins");
}

export async function createTriggerPlugin(request: {
  id: string;
  name: string;
  description?: string;
  runtime_image: string;
  command: string[];
  config_schema?: ParameterContract;
}) {
  return apiFetch<TriggerPlugin>("/v1/trigger-plugins", {
    method: "POST",
    body: JSON.stringify(request)
  });
}

export async function triggerAutomation(id: string) {
  return apiFetch<WorkflowRun>(`/v1/automations/${encodeURIComponent(id)}/trigger`, {
    method: "POST",
    body: "{}"
  });
}

export async function listWorkflowRuns(query: TableQuery = {}) {
  return apiFetch<{ workflow_runs: WorkflowRun[] }>(`/v1/workflow-runs${queryString(query)}`);
}

export async function getRun(id: string) {
  return apiFetch<JobRun>(`/v1/jobs/runs/${encodeURIComponent(id)}`);
}

export async function getRunLogs(id: string) {
  return apiFetch<LogsResponse>(`/v1/jobs/runs/${encodeURIComponent(id)}/logs`);
}

export async function getWorkflowRunLogs(id: string) {
  return apiFetch<WorkflowRunLogsResponse>(`/v1/workflow-runs/${encodeURIComponent(id)}/logs`);
}

export async function removeWorkflowRun(id: string) {
  return apiFetch<WorkflowRun>(`/v1/workflow-runs/${encodeURIComponent(id)}/remove`, {
    method: "POST",
    body: "{}"
  });
}

export async function cancelWorkflowRun(id: string) {
  return apiFetch<WorkflowRun>(`/v1/workflow-runs/${encodeURIComponent(id)}/cancel`, {
    method: "POST",
    body: "{}"
  });
}

export async function listArtifacts(id: string) {
  return apiFetch<{ artifacts: Artifact[] }>(`/v1/jobs/runs/${encodeURIComponent(id)}/artifacts`);
}

export async function submitRun(request: SubmitRunRequest) {
  return apiFetch<JobRun>("/v1/jobs/runs", {
    method: "POST",
    body: JSON.stringify(request)
  });
}

export async function cancelRun(id: string) {
  return apiFetch<JobRun>(`/v1/jobs/runs/${encodeURIComponent(id)}/cancel`, {
    method: "POST",
    body: "{}"
  });
}

export async function downloadArtifact(runId: string, artifact: Artifact) {
  const response = await fetch(
    `/api/capsulet/v1/jobs/runs/${encodeURIComponent(runId)}/artifacts/${encodeURIComponent(artifact.id)}`,
    { cache: "no-store" }
  );

  if (!response.ok) {
    let message = `Download failed with ${response.status}`;
    try {
      const body = (await response.json()) as { code?: string; message?: string };
      message = body.code ? `${body.code}: ${body.message || message}` : body.message || message;
    } catch {
      message = response.statusText || message;
    }
    throw new Error(message);
  }

  const blob = await response.blob();
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = artifact.name;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}

export function isTerminalStatus(status: RunStatus) {
  return status === "succeeded" || status === "failed" || status === "cancelled" || status === "timed_out";
}

export function formatBytes(size: number) {
  if (size < 1024) {
    return `${size} B`;
  }
  if (size < 1024 * 1024) {
    return `${(size / 1024).toFixed(1)} KiB`;
  }
  return `${(size / (1024 * 1024)).toFixed(1)} MiB`;
}
