# Release Process

Capsulet has one release identity, declared in [`release.toml`](../../release.toml) at the
repository root:

- the product name, version, source repository and license;
- the image registry and namespace;
- each component's image name, and the Dockerfile and target it is built from;
- the range of API versions this release's clients are built against.

Everything else that carries a version, a name or a source link is derived from that file, and
`cargo run -p capsulet-xtask -- release check` fails on anything that disagrees. The `release`
gate runs the check in both verification profiles, and CI runs it with the product contracts.

## Images

Each component ships as `<registry>/<namespace>/<image>`:

| Component | Image | Built from |
| --- | --- | --- |
| `api` | `ghcr.io/mohripan/capsulet-api` | `crates/Dockerfile`, target `api` |
| `worker` | `ghcr.io/mohripan/capsulet-worker` | `crates/Dockerfile`, target `worker` |
| `scheduler` | `ghcr.io/mohripan/capsulet-scheduler` | `crates/Dockerfile`, target `scheduler` |
| `graph-worker` | `ghcr.io/mohripan/capsulet-graph-worker` | `crates/Dockerfile`, target `graph-worker` |
| `evaluator` | `ghcr.io/mohripan/capsulet-evaluator` | `crates/Dockerfile`, target `evaluator` |
| `dashboard` | `ghcr.io/mohripan/capsulet-dashboard` | `dashboard/Dockerfile` |

A release is tagged with the product version, for example `ghcr.io/mohripan/capsulet-api:0.1.0`,
and is deployed by digest once it has been pushed. No release or test contract uses `latest`.
Builds from `main` and from pull requests are tagged with the branch, pull request or commit, never
with a version.

The image name is one segment under the namespace. A name assembled from
`${{ github.repository }}` gains a second `capsulet` segment, which is how the images were once
published under a doubled path; the check refuses any image reference under the release's
namespace, and any registry reference in a workflow, that is not one of the names above.

There is one Dockerfile per build strategy, and the check refuses any other:

- `crates/Dockerfile` builds every Rust component. Its `build` stage compiles all five binaries in
  one cargo invocation, and each component is a target on the same non-root runtime base. Building
  several targets in one BuildKit session, as Compose and the `containers` gate do, compiles the
  workspace once.
- `dashboard/Dockerfile` builds the Next.js standalone server on a runtime with npm removed, as a
  non-root user.

Every image is labelled from four build arguments: `VERSION`, `REVISION` (the commit, suffixed
`-dirty` when the worktree had changes), `CREATED` (that commit's time, so rebuilding a commit
gives the same labels) and `SOURCE`. They become `org.opencontainers.image.version`, `.revision`,
`.created`, `.source` and `.url`, next to `.title` (the image name) and `.licenses`.

`cargo run -p capsulet-xtask -- release images --plan` prints each component's image name,
Dockerfile, target and build arguments as a GitHub Actions matrix. The supply-chain workflow builds
from that matrix, and the `containers` gate builds the same plan locally, checks every label and
the non-root user, and removes what it built.

## Derived files

| File | What it carries |
| --- | --- |
| `Cargo.toml` | `[workspace.package]` version, repository and license, and `publish = false`; every crate inherits all four |
| `Cargo.lock` | each workspace member's version |
| `sdk/python/src/capsulet/_version.py` | the SDK's `__version__` and `SUPPORTED_API_VERSIONS`; `pyproject.toml` reads the version from it |
| `dashboard/package.json`, `dashboard/package-lock.json` | the dashboard's version, which its `/healthz` reports |
| `charts/capsulet/Chart.yaml` | the chart `version` and `appVersion`, and its source links |
| `charts/capsulet/values.yaml` | `image.registry`, `image.namespace`, `image.tag` and each component's `image.name` |
| `crates/api/openapi.json` | `info.version` |
| `compose.yaml` | each built service's Dockerfile, target and label arguments |
| `crates/Dockerfile`, `dashboard/Dockerfile` | each component's stage and labels |

The check also holds the third-party images to the same rule: the chart's PostgreSQL, SeaweedFS and
test images, and every image `compose.yaml` pulls, name a tag other than `latest` and are pinned by
digest. Compose's `postgres` and `seaweedfs` run the chart's images, by the same digest. Every link
to this product's GitHub repository, in any tracked file, is the one source.

Some versions are served rather than written down, and are tested where they are served:

- the API's `/healthz`, `/livez` and `/readyz` report `version`;
- `capsulet --version` reports the CLI's;
- the dashboard's `/healthz` reports its own;
- every verifier stack, once healthy, must report `release.toml`'s version from the API and the
  dashboard it built.

`release check-chart`, part of the `helm` gate, renders the chart twice: with the default values
every Capsulet workload must run `<image>:<version>`, and with each component's `image.digest` set
it must run exactly that digest. Every other container must run a pinned dependency.

## Changing the version

```powershell
cargo run -p capsulet-xtask -- release set 0.2.0 --dry-run
cargo run -p capsulet-xtask -- release set 0.2.0 --write
```

`--dry-run` names every file that would change and writes nothing. `--write` rewrites each derived
value in place, leaving every other byte alone, then runs the check; running it again is a no-op.
It refuses a worktree with uncommitted changes, so the version change can be reviewed on its own;
`--allow-dirty` is the release maintainer's explicit override.

A version outside `[compatibility] api` is refused. Widening that range is a deliberate edit to
`release.toml`, because it says which API versions existing clients may meet.

## Cutting a release

1. On a clean `main`, run `release set <version> --write` and open a pull request with the result
   alone.
2. Once it merges and every required gate passes, tag the merge commit `v<version>` and push the
   tag.

`release.yml` then publishes the release:
- It builds every image once and pushes it by digest.
- It scans, attests, signs and verifies each digest, and only then tags it `<version>`.
- It pushes the chart to `oci://ghcr.io/mohripan/charts`, attested and signed.
- It writes the attested `release-manifest.json`.

A tag that does not match `release.toml`'s version fails before anything is tagged. Each push to
`main` runs the same pipeline, tagging the images `main` and `sha-<commit>`, with no version and
no chart push. The [supply chain](../security/supply-chain.md) describes each step, and how to
verify a published image yourself.

Checkpoint tags such as `engineering-integrity/task-04-postgres` mark implementation history, not
product releases.
