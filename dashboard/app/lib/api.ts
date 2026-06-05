export type RunStatus =
  | "queued"
  | "leased"
  | "running"
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
  attempt_count: number;
};

export type LogsResponse = {
  run_id: string;
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
  run_id?: string;
  python_script?: string;
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

export async function listRuns(limit = 50) {
  return apiFetch<{ runs: JobRun[] }>(`/v1/jobs/runs?limit=${limit}`);
}

export async function getRun(id: string) {
  return apiFetch<JobRun>(`/v1/jobs/runs/${encodeURIComponent(id)}`);
}

export async function getRunLogs(id: string) {
  return apiFetch<LogsResponse>(`/v1/jobs/runs/${encodeURIComponent(id)}/logs`);
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
