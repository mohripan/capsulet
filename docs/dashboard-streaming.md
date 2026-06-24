# Dashboard Streaming

Capsulet dashboard pages use server-sent events for live refresh triggers instead of client-side short polling.

## Activity Stream

`GET /v1/events/stream` emits `text/event-stream` snapshots for compact dashboard activity state:

- job definition count
- automation count
- job run ids and states
- workflow run ids, states, current step, and step-run states

The stream sends an initial `snapshot` event and then emits another `snapshot` only when the compact state changes. Dashboard pages keep their normal JSON list endpoints as the source of truth; SSE events only tell the page when to re-fetch.

## Frontend Usage

- Automations opens `/api/capsulet/v1/events/stream` and refreshes its normal JSON datasets on `snapshot`.
- Live Logs opens `/api/capsulet/v1/events/stream` for run-list changes and keeps using the existing per-run log SSE endpoint for log snapshots.
- Manual refresh buttons remain available as a fallback when a user pauses streaming or wants an immediate reload.

This keeps the transport one-way and proxy-friendly. WebSockets are not required until Capsulet needs bidirectional collaboration or terminal-style control.

## Job Definitions UI

The Job Definitions list is intentionally card-like rather than a compressed table. Each row has a fixed right-side actions area and wraps metadata below the job identity, so edit/delete controls remain visible at dashboard widths where six table columns would overflow.

The creation form performs local validation before sending API requests:

- name is required
- runtime image is required
- Python script is required
- parameter names are required and unique

