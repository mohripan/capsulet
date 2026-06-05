# Capsulet Dashboard

This is a Next.js dashboard for Capsulet. The runs surface is connected to the live API for run listing, seeded job submission, Python script submission, run detail, cancellation, logs, artifact listing, and artifact download. Other product-shaped pages still use mock data.

## Requirements

- Node.js 20.x
- npm 10.x

The current local environment uses Node.js 20.15.1 and npm 10.7.0.

## Commands

Install dependencies:

```sh
npm install
```

Run the development server:

```sh
npm run dev
```

Point the dashboard at a local API:

```powershell
$env:CAPSULET_DASHBOARD_API_URL = "http://127.0.0.1:8080"
npm run dev
```

Type-check the dashboard:

```sh
npx tsc --noEmit
```

Production build:

```sh
npm run build
```

Run dashboard tests:

```sh
npm test
```

## Routes

- `/`: overview
- `/automations`: automation catalog and trigger builder
- `/workflows`: workflow definitions and lineage graph
- `/runs`: live run queue, seeded job/script submission, and links to run detail
- `/runs/[id]`: live run status, logs, cancellation, artifacts, and artifact download
- `/execution-pools`: pool routing and node placement
- `/artifacts`: object storage and retention view
- `/security`: pod security, network policy, webhook auth, and service accounts
- `/settings`: platform defaults and future configuration surfaces

## Current Build Caveat

`npx tsc --noEmit` passes and all routes serve successfully in development. During recent local testing on Windows, `next build` intermittently hung in Next.js build worker processes after the dashboard was expanded to multiple routes.

For Sprint 001, CI should run:

```sh
npm ci
npx tsc --noEmit
```

The production build issue should be revisited when the frontend stack is hardened. It may be resolved by a Node.js patch update, a Next.js patch update, or reducing build worker concurrency if needed.

## Data Boundary

Mock data lives in `app/mock-data.ts`. API integration keeps route components focused on presentation and moves data fetching behind typed client functions in `app/lib/api.ts`.

The dashboard uses the same-origin route `app/api/capsulet/[...path]/route.ts` as a server-side proxy. Configure the upstream API with `CAPSULET_DASHBOARD_API_URL`; it defaults to `http://127.0.0.1:8080` for local development.
