import { readFileSync } from "node:fs";
import path from "node:path";
import { request, type APIRequestContext, type APIResponse } from "@playwright/test";

/**
 * The stack fixture that `cargo run -p capsulet-xtask -- stack up`, or the
 * verifier's `dashboard-e2e` gate, wrote for the stack under test. Every id,
 * name and credential a spec uses comes from here: the specs repeat none of
 * the seed's literals, and none of these credentials outlives its stack.
 */
export type Role = "admin" | "operator" | "viewer" | "service";
export type ProjectAlias = "alpha" | "beta";
type Named = { id: string; name: string };
type Identity = {
  principal: string;
  kind: "user" | "service_account";
  role: string;
  project: string;
  token: string;
  expires_at_unix: number;
};
type StackFixture = {
  api_url: string;
  dashboard_url: string;
  tenant: string;
  projects: Record<ProjectAlias, Named>;
  resources: Record<
    ProjectAlias,
    Record<"job_definition" | "memory_subgraph" | "canonical_entity" | "graph" | "agent", Named>
  >;
  identities: Record<Role, Identity>;
  dashboard_login: { username: string; password: string };
};

const fixturePath =
  process.env.CAPSULET_FIXTURE_FILE ??
  path.resolve(process.cwd(), "../.capsulet/stack/capsulet-local/fixture.json");

function readFixture(): StackFixture {
  try {
    return JSON.parse(readFileSync(fixturePath, "utf8")) as StackFixture;
  } catch (error) {
    throw new Error(
      `No stack fixture at ${fixturePath} (${String(error)}). Start a stack with: cargo run -p capsulet-xtask -- stack up`
    );
  }
}

export const fixture = readFixture();
export const alpha = fixture.projects.alpha;
export const beta = fixture.projects.beta;

/** Where each role's signed-in browser state is saved; the global teardown deletes them. */
export const AUTH_DIRECTORY = "playwright/.auth";
export function storageState(role: Exclude<Role, "service">) {
  return `${AUTH_DIRECTORY}/${role}.json`;
}

/** The dashboard keeps the selected project here. */
export const PROJECT_STORAGE_KEY = "capsulet.activeProject";

/** An API client acting as `role`, with `project` selected when given. */
export async function apiAs(role: Role, project?: Named): Promise<APIRequestContext> {
  const extraHTTPHeaders: Record<string, string> = {
    Authorization: `Bearer ${fixture.identities[role].token}`
  };
  if (project) extraHTTPHeaders["x-capsulet-project-id"] = project.id;
  return request.newContext({ baseURL: fixture.api_url, extraHTTPHeaders });
}

/** Runs `body` with an API client as `role`, and disposes of it afterwards. */
export async function asRole<T>(
  role: Role,
  project: Named | undefined,
  body: (api: APIRequestContext) => Promise<T>
): Promise<T> {
  const api = await apiAs(role, project);
  try {
    return await body(api);
  } finally {
    await api.dispose();
  }
}

/** The body of a response that must have `status`, with the body in the failure message. */
export async function expectStatus(response: APIResponse, status: number) {
  if (response.status() !== status) {
    throw new Error(
      `${response.url()} answered ${response.status()}, wanted ${status}: ${await response.text()}`
    );
  }
  const text = await response.text();
  return text ? JSON.parse(text) : null;
}

/** Polls `read` until `done` holds for its value, or fails after `seconds`. */
export async function waitFor<T>(read: () => Promise<T>, done: (value: T) => boolean, seconds = 90) {
  const deadline = Date.now() + seconds * 1000;
  for (;;) {
    const value = await read();
    if (done(value)) return value;
    if (Date.now() > deadline) throw new Error(`still waiting after ${seconds}s: ${JSON.stringify(value)}`);
    await new Promise((resolve) => setTimeout(resolve, 1000));
  }
}
