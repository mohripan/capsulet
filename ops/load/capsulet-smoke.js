import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate } from "k6/metrics";

export const options = {
  scenarios: {
    steady_control_plane: {
      executor: "constant-vus",
      vus: Number(__ENV.VUS || 10),
      duration: __ENV.DURATION || "2m"
    }
  },
  thresholds: {
    http_req_failed: ["rate<0.01"],
    http_req_duration: ["p(95)<750", "p(99)<1500"],
    capsulet_submit_duration: ["p(95)<1000"],
    capsulet_api_ok: ["rate>0.99"]
  }
};

const baseUrl = __ENV.CAPSULET_API_BASE_URL || "http://127.0.0.1:8080";
const token = __ENV.CAPSULET_API_TOKEN || "capsulet-local-admin-token-change-me";
const params = {
  headers: {
    authorization: `Bearer ${token}`,
    "content-type": "application/json",
    "x-request-id": `k6-${__VU}`
  }
};
const apiOk = new Rate("capsulet_api_ok");
const submitDuration = new Trend("capsulet_submit_duration");

function get(path) {
  const response = http.get(`${baseUrl}${path}`, params);
  apiOk.add(response.status >= 200 && response.status < 500);
  return response;
}

export default function () {
  check(get("/readyz"), { "ready": (r) => r.status === 200 });
  check(get("/v1/auth/me"), { "principal": (r) => r.status === 200 });
  check(get("/v1/job-definitions?limit=20"), { "definitions": (r) => r.status === 200 });
  check(get("/v1/jobs/runs?limit=20"), { "runs": (r) => r.status === 200 });
  check(get("/metrics"), { "metrics": (r) => r.status === 200 && r.body.includes("capsulet_http_requests_total") });

  if (__ENV.CAPSULET_LOAD_SUBMIT === "true") {
    const started = Date.now();
    const response = http.post(
      `${baseUrl}/v1/jobs/runs`,
      JSON.stringify({
        job_definition_id: __ENV.CAPSULET_LOAD_JOB_DEFINITION_ID || "hello_python",
        execution_pool: __ENV.CAPSULET_LOAD_EXECUTION_POOL || "mini",
        input: { source: "k6", vu: __VU }
      }),
      params
    );
    submitDuration.add(Date.now() - started);
    check(response, { "submit accepted": (r) => r.status === 201 || r.status === 422 });
  }

  sleep(Number(__ENV.SLEEP_SECONDS || 1));
}
