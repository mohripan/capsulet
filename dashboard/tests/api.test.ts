import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { CapsuletApiError, capsuletStreamUrl, formatBytes, getErrorMessage, isTerminalStatus } from "../app/lib/api";

describe("dashboard API helpers", () => {
  it("formats artifact sizes", () => {
    assert.equal(formatBytes(20), "20 B");
    assert.equal(formatBytes(1536), "1.5 KiB");
    assert.equal(formatBytes(2 * 1024 * 1024), "2.0 MiB");
  });

  it("classifies terminal run statuses", () => {
    assert.equal(isTerminalStatus("queued"), false);
    assert.equal(isTerminalStatus("running"), false);
    assert.equal(isTerminalStatus("retry_scheduled"), false);
    assert.equal(isTerminalStatus("succeeded"), true);
    assert.equal(isTerminalStatus("failed"), true);
    assert.equal(isTerminalStatus("cancelled"), true);
    assert.equal(isTerminalStatus("timed_out"), true);
  });

  it("renders API errors with code and message", () => {
    const error = new CapsuletApiError("job artifact not found", 404, "job_artifact_not_found");
    assert.equal(getErrorMessage(error), "job_artifact_not_found: job artifact not found");
  });

  it("builds proxied stream URLs", () => {
    assert.equal(capsuletStreamUrl("/v1/events/stream"), "/api/capsulet/v1/events/stream");
    assert.equal(capsuletStreamUrl("v1/events/stream"), "/api/capsulet/v1/events/stream");
  });
});
