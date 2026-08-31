# Public Contracts

Capsulet's public-contract layer keeps product language tied to inspectable evidence.

- `product-claims.json` is the authority for public claim maturity and evidence.
- `product-claims.schema.json` defines its stable machine-readable shape.
- `product-claims.md` is deterministic generated output for human readers.
- `lifecycle-and-assurance.md` defines independent execution and assurance semantics.
- `lifecycle-mapping.json` inventories current code statuses and transition ownership.
- `stability-and-versioning.md` defines public classes, deprecation, releases, and readers.
- `database-migrations.md` defines forward upgrade, backup, restore, and rollback obligations.
- `sdk-generation.md` defines generated transports and handwritten ergonomic boundaries.

Claim IDs are permanent and are never reused after retirement. Run
`scripts/check-product-claims.ps1` after changing a public claim or surface, and regenerate the
Markdown view with `scripts/render-product-claims.ps1`.
