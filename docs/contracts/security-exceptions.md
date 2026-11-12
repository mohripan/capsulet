# Security Exceptions

Status: experimental. Every advisory the `security`, `secrets` and `dependency-images` gates are
told to ignore is listed here, with why it does not apply, who owns it, and the date the reasoning
must be re-checked.

`security-exceptions.json` is the structured registry, with exact package versions, scope,
owner, rationale, compensating control, issue and expiry. The gate validates it at runtime;
Rust tool ignores must also match the registry. This Markdown view provides review context.

`xtask security npm` preserves raw output in `.capsulet/security/npm-audit.json`, prints any exception
and checks its ownership, expiry and reachability. A policy pass is **not** a zero-advisory scanner
result. Production dependencies must pass an unexcepted audit.

An exception with no owner is nobody's problem, and an exception with no expiry is permanent by
accident. `crates/xtask/tests/verify_cli.rs` fails the build when `.cargo/audit.toml` ignores an
advisory this file does not list, and when a listed review date has passed. So an exception cannot
be added quietly and cannot outlive its reasoning.

An exception is a statement that an advisory does not apply here — never that it does not matter.
Anything genuinely reachable gets fixed or the gate stays red.

<!-- machine-readable: id | owner | review-by -->

| Advisory | Package | Owner | Review by | Why it does not apply |
| --- | --- | --- | --- | --- |
| `RUSTSEC-2024-0449` | `unsafe-libyaml` 0.2.11 | platform | 2027-03-01 | An unmaintained-crate warning, not a vulnerability. It is reached through `serde_yaml`, which the workspace uses only for the execution pool configuration an operator writes, never a request or a run, and which `kube-client` uses for kubeconfig. Replacing the workspace's own use would not remove it while `kube-client` depends on it; review kube's YAML dependency before this expires. |
| `RUSTSEC-2023-0071` | `rsa` 0.9.10 | platform | 2027-03-01 | The Marvin timing side-channel needs the code to run. `rsa` reaches `Cargo.lock` only through `sqlx-mysql`, and the workspace pins `sqlx` with `default-features = false` and a postgres-only feature set, so `sqlx-mysql` is never compiled. `cargo tree -i rsa` reports nothing, and a workspace-wide `cargo tree -e normal` contains no `rsa` or `sqlx-mysql` node. `cargo audit` reads the lockfile rather than the build graph, so it cannot see that. The advisory has no fixed release, so the alternative to this exception is not a fix — it is either a red gate or dropping sqlx. Revisit when sqlx stops declaring the optional MySQL backend or a patched `rsa` ships; tracked in [#143](https://github.com/mohripan/capsulet/issues/143). |
| `SECRET-2026-0001` | `crates/api/src/auth.rs`, four gitleaks fingerprints in commits `9dc3d50` and `d8b5452` | platform | 2027-05-12 | Unit-test bearer tokens such as `0123456789abcdef0123456789abcdef`, which `AuthConfig`'s tests parse and authenticate; no deployment ever accepted them. A gitleaks fingerprint names the commit, file, rule and line, so the entry covers exactly those occurrences: the same string added by any later commit is a new finding and fails the `secrets` gate, and the gate also fails if one of these stops being found. See the [supply chain](../security/supply-chain.md). |
| `RUSTSEC-2026-0459` | `libsqlite3-sys` 0.30.1 | platform | 2027-03-01 | A CVE in the bundled SQLite, which needs the code to run. `libsqlite3-sys` reaches `Cargo.lock` only through `sqlx-sqlite`, because `sqlx-macros-core` lists every driver as optional; the workspace enables only PostgreSQL, so it is never compiled. `every_uncompiled_exception_is_uncompiled` fails if that changes. The fix (0.39) needs a newer sqlx major; tracked in [#143](https://github.com/mohripan/capsulet/issues/143). |
| `IMAGE-2026-0001` | `chrislusf/seaweedfs` 4.53: `CVE-2026-78661` in `golang.org/x/net` v0.61.0 (`usr/bin/weed`) | platform | 2027-02-12 | A denial of service in `golang.org/x/net/html`, flagged because `weed` is built with that `x/net`. The HTML parser is not in the binary: `weed` links nine `x/net` packages, not `html`, and `govulncheck -mode binary` reports GO-2026-6860 at module level only. Capsulet sends SeaweedFS only S3 requests, and the chart and Compose run `weed mini` with only the S3 port reachable. 4.53 is the newest release. |
| `IMAGE-2026-0002` | `postgres` 16-alpine: 26 Go standard library advisories in `usr/local/bin/gosu` (Go 1.24.6) | platform | 2027-02-12 | `gosu` runs once, in the entrypoint, to drop root before it execs postgres; it opens no socket and parses no network input. `govulncheck -mode binary` on it reaches none of the 26: 25 at module level, `CVE-2026-39822` at package level, no affected function called. The registry lists the ids. |
| `IMAGE-2026-0003` | `postgres` 16-alpine: 16 `krb5-libs` 1.22.2-r1 and 3 `libxslt` 1.1.43-r3 advisories | platform | 2026-11-26 | Alpine shipped the fixes after the image's 2026-11-06 build, and no newer digest exists yet. Nothing Capsulet configures uses GSSAPI or XSLT. Ten krb5 advisories are in the KDC and kadmind, whose libraries postgres does not link; the PKINIT, CAMMAC and NegoEx ones need a KDC, a Kerberos principal or a mechanism in `/etc/gss/mech`, none of which the image has. The two ASN.1 decoder advisories are in `libkrb5`, which postgres links for GSSAPI, but there is no `gss` rule in `pg_hba.conf` and no keytab, so the server has no Kerberos acceptor credentials. `libxslt` is linked only by the `xml2` extension, which no migration creates. Short-lived: move the pin, in Compose and the chart, once the image is rebuilt. |
| `IMAGE-2026-0004` | `prom/prometheus` v3.16.0: `CVE-2026-78661` in `golang.org/x/net` v0.61.0 (`bin/prometheus`, `bin/promtool`) | platform | 2027-02-12 | The same `x/net/html` advisory as `IMAGE-2026-0001`. Neither binary links `golang.org/x/net/html`, and govulncheck reports it at module level only in both. v3.16.0 is the newest release; it runs only in the Compose development stack, on 127.0.0.1. |
| `IMAGE-2026-0005` | `grafana/grafana` 12.4.15: `CVE-2026-21728`, `CVE-2026-28377` in `github.com/grafana/tempo` | platform | 2027-02-12 | Both are in the Tempo server: its search limits and its `/status/config` endpoint. Grafana links only Tempo's protobuf types (`pkg/tempopb`), not its querier, frontend or HTTP endpoints; govulncheck cannot tell, because neither advisory names a package. Compose provisions only a Prometheus datasource, and Grafana runs only in the Compose development stack, on 127.0.0.1. |
| `IMAGE-2026-0006` | `quay.io/keycloak/keycloak` 26.8.2: `CVE-2026-102554` in `guava` 33.6.0-jre | platform | 2026-12-12 | Unbounded allocation while Java-deserializing Guava collections. Keycloak accepts no Java-serialized objects over HTTP, and `start-dev` keeps its caches local, so no cluster peer sends it serialized entries. It is the Compose stack's test identity provider, on 127.0.0.1; the chart does not ship it. 26.8.2 is the newest release, and a patch is expected to move Guava, hence the short expiry. |

## What is not an exception

Advisories that were fixed rather than excused, kept here so the distinction stays visible:

| Advisory | Crate | What happened |
| --- | --- | --- |
| `RUSTSEC-2026-0258` | `h2` | Reachable through `hyper` and `axum`, so it was fixed: `cargo update -p h2` moved 0.4.14 to 0.4.19. |
| `RUSTSEC-2026-0221` | `event-listener` | Unsound `!Send` handling, reached through `sqlx-core`. Fixed by `cargo update`. |
| yanked release | `chacha20` | 0.10.1 was yanked; `cargo update` moved to 0.10.2. |

## Container images

`trivy.yaml` is the image-scanning policy, and it carries one exception of its own: `ignore-unfixed`.

| Policy | Owner | Review by | Why |
| --- | --- | --- | --- |
| `ignore-unfixed` in `trivy.yaml` | platform | 2027-03-01 | Every HIGH and CRITICAL finding in the runtime image today is one Debian has marked `affected`, `fix_deferred`, or `will_not_fix` — 94 of them across 27 packages, and not one has a fixed version to move to. Failing the build on those does not make the image safer; it makes the scan permanently red and therefore unread. What remains after the filter is the actionable set: a published fix this image has not picked up, which one rebuild removes. The unfixed findings are still reported in full to GitHub code scanning by a separate step, so nothing is hidden — only the build-failing threshold is narrowed. Revisit when the base image moves to trixie, which resets the set. |

The runtime image runs `apt-get upgrade` before installing, so a fix Debian publishes lands on the
next build rather than waiting for someone to notice. That is what makes a fixable finding a real
defect rather than a fact about how old the base tag is.

### Images Capsulet pulls

The `dependency-images` gate holds the third-party images in `charts/capsulet/values.yaml` and
`compose.yaml` to the same `trivy.yaml` thresholds, by the digest each is pinned to. Capsulet
cannot rebuild those, so the fix for a finding is a newer pin, in both files alike. When no release
of the image has the fix yet, the finding becomes an `image` exception: `package` is the image
repository, `versions` lists the vulnerability ids, and the scope is `upstream-image`. The gate
fails on any other finding, on an expired entry, and on an entry it no longer needs, which is what
happens once a pin that fixes it lands. See the [supply chain](../security/supply-chain.md#dependency-images).
