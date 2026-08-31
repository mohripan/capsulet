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

function pathParameters(path: string) {
  return [...path.matchAll(/\{([^}]+)\}/g)].map((match) => match[1]).sort();
}

function assertComponentShape(name: string, clientMethod: string) {
  const schema = document.components.schemas[name] as JsonObject;
  assert.ok(schema, `${clientMethod}: missing component ${name}`);
  if (schema.type !== "object") return;
  const properties = schema.properties as JsonObject;
  assert.ok(properties, `${clientMethod}: ${name} object properties`);
  for (const field of schema.required ?? []) {
    assert.ok(properties[field], `${clientMethod}: ${name}.${field} is required but undefined`);
  }
  for (const [field, property] of Object.entries(properties) as [string, JsonObject][]) {
    const types = Array.isArray(property.type) ? property.type : [property.type];
    if (types.includes("null")) {
      assert.ok(types.some((type) => type !== "null"), `${clientMethod}: ${name}.${field} is only null`);
    }
  }
}

describe("dashboard OpenAPI contract", () => {
  it("resolves every explicit handwritten client operation to one contract", () => {
    assert.ok(Object.keys(DASHBOARD_OPERATIONS).length >= 60);
    const operationIds = new Set<string>();

    for (const [clientMethod, expected] of Object.entries(DASHBOARD_OPERATIONS)) {
      const operation = document.paths[expected.path]?.[expected.method.toLowerCase()] as JsonObject | undefined;
      assert.ok(operation, `${clientMethod}: missing ${expected.method} ${expected.path}`);
      assert.equal(operation.operationId, expected.operationId, clientMethod);
      assert.ok(!operationIds.has(operation.operationId), `${clientMethod}: duplicate operationId`);
      operationIds.add(operation.operationId);
      assert.equal(operation["x-capsulet-project-context"], expected.projectContext, clientMethod);

      const parameters = (operation.parameters ?? []) as JsonObject[];
      const declaredPathParameters = parameters
        .filter((parameter) => parameter.in === "path" && parameter.required === true)
        .map((parameter) => parameter.name)
        .sort();
      assert.deepEqual(declaredPathParameters, pathParameters(expected.path), `${clientMethod}: path parameters`);
      assert.ok(
        parameters.some((parameter) => parameter.name === "x-request-id" && parameter.in === "header"),
        `${clientMethod}: request-id header`
      );

      const requestSchema = schemaName(operation.requestBody?.content?.["application/json"]?.schema);
      assert.equal(requestSchema, expected.requestSchema, `${clientMethod}: request schema`);
      assert.equal(Boolean(operation.requestBody), expected.requestSchema !== null, `${clientMethod}: request body`);
      if (expected.requestSchema) {
        assert.equal(operation.requestBody.required, true, `${clientMethod}: required request body`);
        assertComponentShape(expected.requestSchema, clientMethod);
      }

      const success = Object.entries(operation.responses as JsonObject)
        .find(([status]) => status.startsWith("2"))?.[1] as JsonObject | undefined;
      assert.ok(success, `${clientMethod}: success response`);
      const content = Object.values(success.content ?? {})[0] as JsonObject | undefined;
      assert.equal(schemaName(content?.schema), expected.responseSchema, `${clientMethod}: response schema`);
      const contentTypes = Object.keys(success.content ?? {});
      const expectedContentType = expected.responseSchema === "BinaryResponse"
        ? "application/octet-stream"
        : expected.responseSchema === "EventStreamResponse"
          ? "text/event-stream"
          : expected.responseSchema === null
            ? null
            : "application/json";
      assert.deepEqual(contentTypes, expectedContentType ? [expectedContentType] : [], `${clientMethod}: media type`);
      if (expected.responseSchema) assertComponentShape(expected.responseSchema, clientMethod);

      assert.equal(
        schemaName(operation.responses.default?.content?.["application/json"]?.schema),
        "Error",
        `${clientMethod}: shared error envelope`
      );

      if (expected.projectContext) {
        assert.ok(
          (operation.parameters as JsonObject[]).some(
            (parameter) => parameter.name === "x-capsulet-project-id" && parameter.in === "header"
          ),
          `${clientMethod}: project header`
        );
      }
      assert.equal(
        parameters.some((parameter) => parameter.name === "x-capsulet-project-id" && parameter.in === "header"),
        expected.projectContext,
        `${clientMethod}: project header presence`
      );
    }
  });
});
