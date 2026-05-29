# ADR 0003: Next.js Dashboard Prototype

Status: Accepted

## Context

Capsulet needs a visual dashboard prototype before backend APIs exist. The dashboard should communicate the intended product surface: automations, workflows, runs, execution pools, artifacts, security, and settings.

## Decision

Use Next.js with TypeScript for the dashboard prototype.

The Sprint 001 dashboard is frontend-only and uses mock data in `dashboard/app/mock-data.ts`. It should remain easy to discard, refactor, or wire to real APIs later.

YAML import and export are API and CLI responsibilities for now. The dashboard should use structured forms and visual builders rather than raw YAML editing in early versions.

## Consequences

- The product shape can be reviewed visually before backend implementation.
- The dashboard can later become the real frontend without changing the repository layout.
- Mock data must stay isolated so backend integration does not require a broad rewrite.
- CI should type-check the dashboard. Production build hardening can happen after the frontend stack stabilizes.
