# Ingestion Review Queue Design

## Status

Approved for implementation.

## Goal

Capsulet ingestion should propose memory, not silently trust it. The first review queue exposes ingested candidate claims and lets a reviewer approve or reject each claim.

## Backend

Use the existing `ClaimStatus` model:

- `candidate` means proposed memory awaiting review.
- `active` means approved trusted memory.
- `rejected` means reviewer rejected the proposed claim.

Add API endpoints:

- `GET /v1/ingestion/review/claims`
- `POST /v1/ingestion/review/claims/{id}/approve`
- `POST /v1/ingestion/review/claims/{id}/reject`

The list endpoint is project scoped and returns reviewable claim records with claim fields, status, confidence, authority, evidence IDs, and source/evidence context where available.

Approve and reject load the claim, change its status, and persist it through the existing memory-claim store path.

## Frontend

Add a review inbox to `/memory/ingestion`.

The inbox shows candidate, active, and rejected claims with simple status filters and approve/reject actions for candidate claims.

## Testing

Tests cover:

- candidate claims appear in the review queue
- approving a candidate claim changes status to `active`
- rejecting a candidate claim changes status to `rejected`
- dashboard API helpers call the expected review endpoints
- dashboard build succeeds with the review inbox
