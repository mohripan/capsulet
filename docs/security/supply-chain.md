<!-- capsulet-claims: CAP-SUPPLY-001 -->
# Supply Chain

Capsulet publishes what [`release.toml`](../../release.toml) names: one image per component, the
Helm chart and a release manifest. Each image is built once, and the digest that build pushed is
the only thing that is scanned, described, attested, signed, verified and tagged. Pull requests
build and scan the same images without any credential that could publish, sign or attest.

`cargo run -p capsulet-xtask --locked -- verify --gate supply-chain-contract` holds the workflows
to this document, and its tests show each rule failing on a workflow that breaks it.

## Two pipelines

| | Pull request | `main` and `v*` tags |
| --- | --- | --- |
| Workflow | `supply-chain.yml` → `image-scan.yml` | `release.yml` → `image-release.yml` |
| Permissions | read, and uploading its own scan results | `packages`, `id-token` and `attestations` write, in the jobs that publish |
| Image | built once into the runner's Docker store | built once and pushed by digest, with no tag |
| After the build | labels and user checked, scanned, SBOM | the same, then attested, signed, verified and only then tagged |

Both pipelines take each component's image name, Dockerfile, target and label arguments from
`release images --plan`. Neither writes that matrix down, so they cannot name or label an image
differently from each other or from the [`containers` gate](../contracts/release-process.md).

`pull_request_target` is never used, because it would run a pull request's code with the
repository's secrets. A pull request's workflows may read no secret but the default token, and
may not inherit secrets. They also reach no reusable workflow that holds a write permission other
than `security-events`.

## One image, one digest

`image-release.yml` runs these steps for each component:

1. **Build and push once**, with `push-by-digest=true` and no tag. A digest that never passes
   the steps below stays untagged and unsigned, and nothing points at it.
2. **Pull that digest back and check it**: `release inspect` requires the OCI labels of this
   checkout's release identity and a non-root user.
3. **Scan the digest twice.** A report of every finding goes to code scanning. A gate fails the
   job under [`trivy.yaml`](../../trivy.yaml): a fixable HIGH or CRITICAL vulnerability, a secret,
   or a HIGH misconfiguration in the image's own configuration. The policy's one exception,
   `ignore-unfixed`, is owned and expires in the
   [exceptions document](../contracts/security-exceptions.md).
4. **Describe it**: a CycloneDX SBOM of the digest.
5. **Attest**:
   - SLSA v1 build provenance, which names the repository, the commit, the workflow, the runner,
     the triggering event and its inputs, and the digest;
   - the SBOM, for the same digest.

   Both are signed through Sigstore with the workflow's identity and pushed to the registry next
   to the image.
6. **Sign the digest** with cosign, keyless.
7. **Verify all of it before anything is tagged**:
   - the signature, issued to GitHub Actions for `image-release.yml` at this ref;
   - the provenance and the SBOM, attested for this digest by that workflow;
   - that the provenance names this repository and this commit.
8. **Tag the verified digest** with `sha-<commit>`, and with `main` on `main` or `<version>` on a
   `v<version>` tag. Every tag must then resolve to the verified digest. A tag that does not
   match `release.toml`'s version fails.

After every image, `release.yml`:

- packages the chart, attests the package's provenance and signs it as a blob;
- on a version tag, also pushes the chart to `oci://ghcr.io/mohripan/charts`, and attests and
  signs that digest;
- writes a CycloneDX SBOM of the source packages: `Cargo.lock`, the dashboard's lockfile and the
  Python SDK;
- verifies every image's signature, provenance and SBOM again, from the registry;
- writes `release-manifest.json`, which names each image by digest and its tags, and the chart
  and the source SBOM by SHA-256, with the revision they were built from;
- attests the manifest.

The manifest is the run's artifact; it is not written until everything it names is verified.

## Verifying an image yourself

```sh
cosign verify ghcr.io/mohripan/capsulet-api@sha256:<digest> \
  --certificate-identity "https://github.com/mohripan/capsulet/.github/workflows/image-release.yml@refs/heads/main" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com
gh attestation verify oci://ghcr.io/mohripan/capsulet-api@sha256:<digest> \
  --repo mohripan/capsulet \
  --signer-workflow mohripan/capsulet/.github/workflows/image-release.yml \
  --predicate-type https://slsa.dev/provenance/v1
gh attestation verify oci://ghcr.io/mohripan/capsulet-api@sha256:<digest> \
  --repo mohripan/capsulet \
  --signer-workflow mohripan/capsulet/.github/workflows/image-release.yml \
  --predicate-type https://cyclonedx.org/bom
```

For a released version, use `refs/tags/v<version>` in place of `refs/heads/main`. Deploy by
digest: the chart's `<component>.image.digest` pins each workload to exactly the image that was
verified.

## Pins

| What | Pinned by | Kept current by |
| --- | --- | --- |
| GitHub Actions | full commit SHA, with a `# vX.Y.Z` comment | Dependabot (`github-actions`), which updates the SHA and the comment together |
| Base images in `crates/Dockerfile` and `dashboard/Dockerfile` | `name:tag@sha256:digest` | Dependabot (`docker` in `/crates` and `/dashboard`) |
| Images Compose and the chart pull | digest, the same in both (`release check`), and scanned by that digest ([`dependency-images`](#dependency-images)) | review |
| Trivy | the version the pinned `trivy-action` installs; the `dependency-images` gate runs the same release by image digest, in `crates/xtask/src/security/images.rs` | its Dependabot update for the action, review for the image |
| gitleaks | image digest, in `crates/xtask/src/security.rs` | review |
| cargo-audit, cargo-deny | exact versions in `supply-chain.yml` | review |

A tag can be moved to other code after it was reviewed; a commit SHA or a digest cannot. This
happened to `trivy-action`'s own tags in 2026. The rebuilt Debian runtime still runs `apt-get
upgrade`, so a pinned base image does not freeze its security fixes. Dependabot proposes newer
pins as pull requests, which go through the same unprivileged checks.

## Secrets

The `secrets` gate runs gitleaks, pinned by digest, over the whole Git history: CI checks out
with `fetch-depth: 0`. A secret removed from the tree is still in every clone of the commit that
added it. The report is redacted and records where a secret is, never what it is.

Every finding must be a registered test fixture in
[`security-exceptions.json`](../contracts/security-exceptions.json): ecosystem `secret`, scope
`test-fixture`, an owner, an issue and an expiry. An entry lists gitleaks fingerprints, each of
which names the commit, file, rule and line. So it covers exactly the occurrences it names, and
the same string added by a later commit is a new finding. A full scan also fails when a
registered fingerprint is no longer found.

Before committing, scan what is staged:

```powershell
cargo run -p capsulet-xtask -- security secrets --staged
```

To run it on every commit, add it to `.git/hooks/pre-commit`:

```sh
#!/bin/sh
exec cargo run --quiet -p capsulet-xtask -- security secrets --staged
```

A staged finding always fails, since nothing staged has a commit to register. Build test tokens at
runtime rather than writing a secret-shaped literal; if one really must be committed, commit it,
and register the fingerprint the CI scan reports.

## Dependency images

Capsulet also ships images it does not build. The chart bundles PostgreSQL, SeaweedFS and the
image `helm test` runs, and Compose adds Mailpit, Keycloak, Prometheus and Grafana. The
`dependency-images` gate scans each one:

- `release dependency-images` lists every image that `charts/capsulet/values.yaml` and
  `compose.yaml` pin. It reads them with the parsers `release check` uses, lists each digest
  once, and names every place that pins it. A Compose service that builds its image is a
  component, which the pipelines above scan.
- Trivy runs in Docker, pinned by digest. It is the release the pinned `trivy-action` installs.
  It downloads its databases once, then reads each image from its registry by the pinned digest,
  taking the `linux/amd64` manifest of a multi-platform image. What is scanned is exactly what the
  chart and Compose run. Each raw report is kept below `.capsulet/security/dependency-images/`.
- The thresholds are `trivy.yaml`'s, read from that file: a HIGH or CRITICAL vulnerability that
  has a fixed version fails.

Capsulet cannot rebuild an upstream image. If a newer release of the image fixes a finding, move
the pin, in `values.yaml` and `compose.yaml` alike. If none does yet, register the finding in
[`security-exceptions.json`](../contracts/security-exceptions.json) with:

- ecosystem `image` and scope `upstream-image`;
- the repository as `package`, such as `chrislusf/seaweedfs`;
- the vulnerability ids as `versions`;
- an owner, an issue and an expiry.

The gate fails on a finding nobody registered and on an expired entry. It also fails on an entry
the scan no longer needs: one whose vulnerability no longer fails the policy, such as after a new
pin, or whose image nothing pulls any more.

```powershell
cargo run -p capsulet-xtask -- verify --gate dependency-images
```

The `dependency images` job in `supply-chain.yml` runs the gate for every pull request and on
`main`.

## Summaries and failures

Every scanning job writes its scanner's version, the policy it applied and the result to the job
summary. Nothing converts a failed scan into a warning:

- no step or job sets `continue-on-error`;
- no scanner command ends in `|| true`;
- a report-only Trivy step is always paired with one that gates.

## What the contract checks

`supply-chain check` parses every workflow under `.github/workflows` and refuses:

- an action not pinned to a full commit SHA, or pinned without its release comment;
- `pull_request_target`, a missing top-level `permissions` in a workflow pull requests run, any
  write permission other than `security-events` there or in a workflow it calls, inherited
  secrets, or a secret other than the default token;
- `continue-on-error`, a scanner turned into a pass, or a report-only scan with no gate;
- a job that builds an image more than once;
- a publishing job that:
  - pushes tags while building, or does not push by digest;
  - scans, describes or attests anything but the digest it pushed;
  - lacks the gate, the provenance, the SBOM attestation, the signature or the verification;
  - or tags before scanning, signing and verifying;
- signing anything by reference other than a digest;
- a release manifest written before every image is verified;
- no CI job that scans the whole history for secrets, or none that runs the `dependency-images`
  gate;
- a base image pinned by tag alone, or a pin Dependabot does not refresh.

## Not provided

- **SLSA level.** The provenance is generated and signed by GitHub's attestation action in the job
  that built the image. That is SLSA Build Level 2: authenticated, but not produced by a builder
  isolated from the build's own steps. Level 3 needs a separate, hardened builder, such as the SLSA
  GitHub generator.
- **Public transparency.** Signatures and attestations are recorded in Sigstore's public
  transparency log, so the workflow identity that signed each digest is public.
- **Old images.** Images pushed before this pipeline, including those under the earlier doubled
  path (a `capsulet/` segment before each image name), are unsigned and have no attestations.
