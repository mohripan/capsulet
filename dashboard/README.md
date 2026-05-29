# Capsulet Dashboard

This is a frontend-only Next.js prototype for the Capsulet dashboard. It uses mock data so the product shape can be reviewed before the backend APIs exist.

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

Type-check the dashboard:

```sh
npx tsc --noEmit
```

Production build:

```sh
npm run build
```

## Mock Routes

- `/`: overview
- `/automations`: automation catalog and trigger builder
- `/workflows`: workflow definitions and lineage graph
- `/runs`: run queue, attempts, logs, and artifacts
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

Mock data lives in `app/mock-data.ts`. Future API integration should keep route components focused on presentation and move data fetching behind typed client functions.
