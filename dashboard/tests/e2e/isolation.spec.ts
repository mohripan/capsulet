import { expect, test, type APIRequestContext } from "@playwright/test";
import { alpha, asRole, beta, expectStatus, fixture, storageState, waitFor, type Role } from "./fixture";

// Every identity but the admin belongs to alpha alone. These flows check, one
// resource family at a time, that what alpha holds is usable by its members
// within their roles, and that beta's is neither visible nor operable from
// alpha nor reachable by selecting beta.

const own = fixture.resources.alpha;
const other = fixture.resources.beta;
const FINISHED = ["succeeded", "failed", "cancelled", "timed_out", "stopped"];

async function runJob(api: APIRequestContext, jobDefinitionId: string) {
  const run = await expectStatus(
    await api.post("/v1/jobs/runs", { data: { job_definition_id: jobDefinitionId, execution_pool: "mini" } }),
    201
  );
  const finished = await waitFor(
    async () => expectStatus(await api.get(`/v1/jobs/runs/${run.id}`), 200),
    (current) => FINISHED.includes(current.status)
  );
  expect(finished.status, `run ${run.id}`).toBe("succeeded");
  const artifacts = await expectStatus(await api.get(`/v1/jobs/runs/${run.id}/artifacts`), 200);
  expect(artifacts.artifacts.length, `artifacts of run ${run.id}`).toBeGreaterThan(0);
  return { run: run.id as string, artifact: artifacts.artifacts[0].id as string };
}

test("each identity is the principal, role and membership the fixture names", async () => {
  for (const role of ["admin", "operator", "viewer", "service"] as const) {
    const identity = fixture.identities[role];
    const me = await asRole(role, undefined, async (api) => expectStatus(await api.get("/v1/auth/me"), 200));
    expect(me.name, role).toBe(identity.principal);
    expect(me.platform_admin, role).toBe(role === "admin");
    if (role !== "admin") {
      expect(me.project_memberships.map((membership: { project_id: string }) => membership.project_id), role).toEqual([alpha.id]);
    }
  }
});

test("only a platform admin sees both projects", async () => {
  const listed = async (role: Role) =>
    (await asRole(role, undefined, async (api) => expectStatus(await api.get("/v1/projects"), 200))).projects.map(
      (project: { id: string }) => project.id
    );
  expect(await listed("admin")).toEqual(expect.arrayContaining([alpha.id, beta.id]));
  for (const role of ["operator", "viewer", "service"] as const) {
    expect(await listed(role), role).toEqual([alpha.id]);
  }
});

test("a job's run and artifact are downloadable in its project and nowhere else", async () => {
  const alphaRun = await asRole("service", alpha, (api) => runJob(api, own.job_definition.id));
  const betaRun = await asRole("admin", beta, (api) => runJob(api, other.job_definition.id));

  await asRole("viewer", alpha, async (api) => {
    const download = await api.get(`/v1/jobs/runs/${alphaRun.run}/artifacts/${alphaRun.artifact}`);
    expect(download.status()).toBe(200);
    expect(await download.text()).toContain(`hello from ${alpha.id}`);
    // Reading is a viewer's; running is not.
    expect(
      (await api.post("/v1/jobs/runs", { data: { job_definition_id: own.job_definition.id, execution_pool: "mini" } })).status()
    ).toBe(403);
    // Beta's run, artifact and job are not found from alpha.
    expect((await api.get(`/v1/jobs/runs/${betaRun.run}`)).status()).toBe(404);
    expect((await api.get(`/v1/jobs/runs/${betaRun.run}/artifacts/${betaRun.artifact}`)).status()).toBe(404);
    expect((await api.get(`/v1/job-definitions/${other.job_definition.id}`)).status()).toBe(404);
  });
  await asRole("operator", beta, async (api) => {
    expect((await api.get("/v1/jobs/runs")).status()).toBe(403);
    expect((await api.get(`/v1/jobs/runs/${betaRun.run}/artifacts/${betaRun.artifact}`)).status()).toBe(403);
  });
});

test("memory is read and reviewed only inside its own project", async () => {
  await asRole("viewer", alpha, async (api) => {
    const subgraph = await expectStatus(await api.get(`/v1/memory/subgraphs/${own.memory_subgraph.id}`), 200);
    expect(subgraph.name).toBe(own.memory_subgraph.name);
    expect((await api.get(`/v1/memory/subgraphs/${other.memory_subgraph.id}`)).status()).toBe(404);
    expect((await api.get("/v1/ingestion/review/claims")).status()).toBe(200);
    expect(
      (await api.post("/v1/memory/canonical-entities", {
        data: { entity_type: "Customer", display_name: `${alpha.name} viewer write`, aliases: [] }
      })).status()
    ).toBe(403);
    const entities = JSON.stringify(await expectStatus(await api.get("/v1/memory/canonical-entities"), 200));
    expect(entities).toContain(own.canonical_entity.id);
    expect(entities).not.toContain(other.canonical_entity.id);
  });
  await asRole("viewer", beta, async (api) => {
    expect((await api.get("/v1/memory/subgraphs")).status()).toBe(403);
    expect((await api.get("/v1/ingestion/review/claims")).status()).toBe(403);
  });
});

test("an agent runs in its own project, for an operator and not a viewer", async () => {
  const run = await asRole("operator", alpha, async (api) => {
    const started = await expectStatus(
      await api.post(`/v1/agents/${own.agent.id}/runs`, { data: { initial_state: { query: "  what   is capsulet? " } } }),
      201
    );
    return waitFor(
      async () => expectStatus(await api.get(`/v1/agent-runs/${started.id}`), 200),
      (current) => FINISHED.includes(current.status)
    );
  });
  expect(run.status).toBe("succeeded");
  expect(run.state.prompt).toBe("Answer this: what is capsulet?");

  await asRole("viewer", alpha, async (api) => {
    expect((await api.get(`/v1/agent-runs/${run.id}`)).status()).toBe(200);
    expect((await api.post(`/v1/agents/${own.agent.id}/runs`, { data: { initial_state: {} } })).status()).toBe(403);
  });
  await asRole("operator", alpha, async (api) => {
    expect((await api.get(`/v1/graphs/${other.graph.id}`)).status()).toBe(404);
    expect((await api.post(`/v1/agents/${other.agent.id}/runs`, { data: { initial_state: {} } })).status()).toBe(404);
  });
  await asRole("operator", beta, async (api) => {
    expect((await api.post(`/v1/agents/${other.agent.id}/runs`, { data: { initial_state: {} } })).status()).toBe(403);
  });
  await asRole("admin", beta, async (api) => {
    expect((await api.get(`/v1/agent-runs/${run.id}`)).status()).toBe(404);
  });
});

test("an audit event is visible only in the project it happened in", async () => {
  await asRole("service", alpha, async (api) => {
    await expectStatus(
      await api.post("/v1/jobs/runs", { data: { job_definition_id: own.job_definition.id, execution_pool: "mini" } }),
      201
    );
  });
  type Event = { principal: string; method: string; path: string };
  const events = async (role: Role, project: typeof alpha) =>
    (await asRole(role, project, async (api) => expectStatus(await api.get("/v1/audit-events"), 200))).audit_events as Event[];

  const service = fixture.identities.service.principal;
  const inAlpha = await events("viewer", alpha);
  expect(inAlpha.some((event) => event.principal === service && event.method === "POST" && event.path === "/v1/jobs/runs")).toBe(true);
  // The seed wrote each project's job definition; each write is audited in its own project only.
  expect(inAlpha.some((event) => event.path.endsWith(own.job_definition.id))).toBe(true);
  expect(inAlpha.some((event) => event.path.endsWith(other.job_definition.id))).toBe(false);

  const inBeta = await events("admin", beta);
  expect(inBeta.some((event) => event.principal === service)).toBe(false);
  expect(inBeta.some((event) => event.path.endsWith(other.job_definition.id))).toBe(true);

  await asRole("operator", beta, async (api) => {
    expect((await api.get("/v1/audit-events")).status()).toBe(403);
  });
});

test.describe("in the dashboard, as a viewer", () => {
  test.use({ storageState: storageState("viewer") });

  test("the project switcher and memory show alpha and nothing of beta", async ({ page }) => {
    await page.goto("/memory");
    await expect(page.getByRole("heading", { name: "Graph Workbench" })).toBeVisible();
    await expect(page.getByText(own.memory_subgraph.name).first()).toBeVisible();
    await expect(page.getByText(other.memory_subgraph.name)).toHaveCount(0);
    await expect(page.getByText(beta.name)).toHaveCount(0);
  });
});
