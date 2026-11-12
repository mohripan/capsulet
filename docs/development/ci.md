# Continuous Integration

CI runs the verifier and nothing else. Every check is a gate of
`cargo run -p capsulet-xtask --locked -- verify`, defined once in
`crates/xtask/src/verify/catalog.rs`. A workflow only chooses which gates a job runs and provides
the tools they need, so a gate passes or fails on a developer's machine exactly as it does in CI.

## The one local command

```powershell
cargo run -p capsulet-xtask --locked -- verify --profile full
```

That runs every required gate: Docker, Kind, kubectl and Helm, among the others. `verify doctor`
reports each tool's version against the supported minimum. `--profile fast` is the quick loop, and
it prints what it leaves out. `verify --gate <name>` runs one gate, and `verify --list` prints them
all with their commands.

## Required jobs

`ci.yml` is the primary workflow. Its jobs:

| Job | Gates |
| --- | --- |
| Rust and IR | `verifier`, `format`, `lint`, `unit`, `ir`, `replay` |
| Product contracts | `claims`, `api-contracts`, `authorization`, `sdk`, `claims-evidence`, `release`, `supply-chain-contract`, `ci-contract` |
| PostgreSQL | `postgres`, `migrations`, `chaos` |
| Dashboard | `dashboard` |
| Helm | `helm` |
| Compose | `compose` |
| Dashboard end-to-end | `dashboard-e2e` |
| Container images | `containers` |
| Dependency policy | `security`, `gate-regressions` |
| Secret scanning | `secrets`, over the whole history |
| Dependency images | `dependency-images` |
| Kind | `kind` |
| Required | none of its own: it checks the others |

**Required** is the one check branch protection requires. It waits for every other job, and runs
even when one of them failed or was skipped, because a skipped required check reads as passing.
Every gate job uploads its run's `verification-summary.json`. Required downloads them all and runs
`verify summary --require full --needs '<the jobs it waited for>'`. That fails unless:

- every gate of the full profile ran in some job and passed;
- no gate that ran reported a failed test;
- every job Required waited for succeeded.

A job that is green without having run its gate cannot make Required pass.

`ci-contract` (`xtask ci check`) holds the workflows to this. It refuses:

- a step that runs a check itself, such as `cargo test`, `helm lint` or `npx playwright test`,
  rather than its gate;
- a gate job or gate step behind an `if:`, which would let it skip silently;
- a matrix in `ci.yml` over anything but platforms or toolchains;
- a gate job that does not upload its summary;
- a full-profile gate no job runs;
- a Required job that does not wait for every job, does not run `if: always()`, or does not check
  the summaries.

`crates/xtask/tests/ci_contract.rs` breaks each rule in a copy of the workflows.

## The other workflows

- **`supply-chain.yml`**, for pull requests: builds and scans every image without publishing it.
  See the [supply chain](../security/supply-chain.md). It holds a code-scanning upload permission
  that `ci.yml` does not, and `release.yml` scans the same images on `main`.
- **`release.yml`**, on `main` and version tags: builds, scans, attests, signs and verifies every
  image by digest, then writes the release manifest.
- **`wasm-python-integration.yml`**, nightly and on demand: runs the `wasm-python` gate, which needs
  Wasmtime and a WASI Python build that no ordinary runner has. It supplements, and does not
  replace, the WASI runner's contract tests in `crates/runner`. Those run in `unit` on every pull
  request:
  - the Wasmtime command line, including the sandbox's preopened directory;
  - refusal of Python dependencies;
  - validation of what an execution may ask for.

## The Kind gate

`kind` installs Capsulet into a disposable Kind cluster and exercises it through Kubernetes
networking alone. It:

1. builds every component's image from the checkout and checks its labels and user;
2. creates a cluster named for the verifier run, with a kubeconfig of its own, so the developer's
   kubeconfig is never read or written, and loads those images into it;
3. generates the test credentials:
   - API tokens for an administrator, and for an operator and a viewer of `kind-alpha`;
   - the dashboard's bootstrap login;
   - the database password;
4. before installing, holds the release to:
   - `helm lint`;
   - the workload rules of `release check-chart`: non-root and seccomp-confined pods, no privilege
     escalation, every capability dropped, read-only root filesystems for Capsulet's own
     containers, service-account tokens only where the pod uses the Kubernetes API, and readiness
     and liveness probes;
   - images that are this run's own or pinned by digest;
   - every secret a pod reads existing;
   - the API server's validation of every manifest, through a server-side dry run;
5. installs the chart, waits for every deployment, and runs `helm test`;
6. runs [`tests/kind/smoke.py`](../../tests/kind/smoke.py) as a Job inside the cluster, against the
   release's Services. It checks:
   - health and the served version;
   - the dashboard login;
   - a project-scoped job run, which the worker executes as a Kubernetes Job, with its logs and
     artifact;
   - an agent run that the graph worker advances under the correctness kernel;
   - denial across projects and roles;
7. uninstalls, and fails if anything the release created remains.

On failure, it writes the namespace's events, pod descriptions and logs to the gate log. In every
case it deletes its cluster and the images it built. The gate's teardown deletes that run's
cluster again if the smoke was killed, and never touches another cluster.

The Kind job installs Kind 0.31.0 and kubectl 1.35.0 by checksum and Helm 4.1.4, and
`tests/kind/cluster.yaml` pins the node image by digest, so a tool release cannot change what the
gate runs without a commit. Locally, `verify doctor` reports each against its minimum.

## Reproducing a failure

1. Find the failed gate in Required's summary, then the job that ran it.
2. Re-run that gate locally:

   ```powershell
   cargo run -p capsulet-xtask --locked -- verify --gate <name>
   ```

3. Read its log at `.capsulet/verify/<run>/<gate>.log`. A failing job prints the last 60 lines, and
   the Dashboard end-to-end and Kind jobs also upload the whole log.

Every artifact a job uploads (gate logs, `verification-summary.json`, `test-counts.json`) comes from
the run that failed, under its run directory.

## Inspecting verification artifacts

`verification-summary.json` records, for each gate a run attempted:

- its status: `passed`, `failed`, `timed_out`, `error` or `missing_prerequisite`;
- its duration;
- the commands it ran;
- what each prerequisite tool reported as its version;
- the verifier's version;
- the passed, failed and ignored tests of every test binary;
- the path and SHA-256 of its log.

Download a run's `verification-*` artifacts and check them as Required does:

```powershell
cargo run -p capsulet-xtask --locked -- verify summary --require full <directory>
```

## Cache boundaries

Caches only save time; nothing a cache holds decides whether a gate passes.

- **Rust:** `Swatinem/rust-cache` restores the target directory and the registry, per job, keyed by
  the toolchain and `Cargo.lock`.
- **Dashboard:** `actions/setup-node` caches npm's downloads, keyed by
  `dashboard/package-lock.json`, and `npm ci` installs from the lockfile every time.
- **Images:** no Docker build cache crosses jobs or runs. Each image job, the `containers` gate and
  the Kind gate build from the pinned base images. The Rust targets of one build share their build
  stage, so each job compiles the workspace once.
- **The verifier's own tests** build in `.capsulet/verify/xtask-tests`, apart from the running
  executable's target directory.
