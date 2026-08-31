import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

import { DASHBOARD_OPERATIONS } from "../app/lib/api";

type JsonObject = Record<string, any>;

const document = JSON.parse(
  readFileSync(new URL("../../crates/api/openapi.json", import.meta.url), "utf8")
) as JsonObject;

function schemaName(schema: JsonObject | undefined) {
  const reference = schema?.$ref as string | undefined;
  return reference?.split("/").at(-1) ?? null;
}

describe("dashboard OpenAPI contract", () => {
  it("resolves every explicit handwritten client operation exactly once", () => {
    assert.ok(Object.keys(DASHBOARD_OPERATIONS).length >= 60);
    const operationIds = new Set<string>();

    for (const [clientMethod, expected] of Object.entries(DASHBOARD_OPERATIONS)) {
      const operation = document.paths[expected.path]?.[expected.method.toLowerCase()] as JsonObject | undefined;
      assert.ok(operation, `${clientMethod}: missing ${expected.method} ${expected.path}`);
      assert.equal(operation.operationId, expected.operationId, clientMethod);
      assert.ok(!operationIds.has(operation.operationId), `${clientMethod}: duplicate operationId`);
      operationIds.add(operation.operationId);
      assert.equal(operation["x-capsulet-project-context"], expected.projectContext, clientMethod);

      const requestSchema = schemaName(operation.requestBody?.content?.["application/json"]?.schema);
      assert.equal(requestSchema, expected.requestSchema, `${clientMethod}: request schema`);

      const success = Object.entries(operation.responses as JsonObject)
        .find(([status]) => status.startsWith("2"))?.[1] as JsonObject | undefined;
      assert.ok(success, `${clientMethod}: success response`);
      const content = Object.values(success.content ?? {})[0] as JsonObject | undefined;
      assert.equal(schemaName(content?.schema), expected.responseSchema, `${clientMethod}: response schema`);

      if (expected.projectContext) {
        assert.ok(
          (operation.parameters as JsonObject[]).some(
            (parameter) => parameter.name === "x-capsulet-project-id" && parameter.in === "header"
          ),
          `${clientMethod}: project header`
        );
      }
    }
  });
});
