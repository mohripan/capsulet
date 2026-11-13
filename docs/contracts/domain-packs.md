# Domain Packs

Status: experimental. A domain pack carries everything domain-specific, as
[ADR 0022](../adr/0022-signed-versioned-domain-packs.md) decided it and `crates/pack` implements it:
contracts, validators with the cases that show they work, and the policy defaults for its
obligations. The kernel and runtime contain none of it.

## The directory

| File | Holds |
| --- | --- |
| `pack.json` | the manifest, `capsulet.pack/v1`: `name`, `version`, `description`, `contracts`, `validators`, `obligations` and `files` |
| `pack.sig` | the signature, `capsulet.pack-signature/v1`: the key, the manifest digest, and the Ed25519 signature over it |
| anything else | listed in `files` by path and SHA-256 digest |

- **Name:** lower-case letters, digits and hyphens, starting with a letter.
- **Version:** semantic.
- **Contracts:** memory contract DSL files, which must parse and compile.
- **Validators:** each a `capsulet.verifier/v1` registration with its cases file
  ([verifier protocol](verifier-protocol.md)). The registration must be one the host may run. A
  validator may say which of the pack's obligations it `decides`: each has to be one the pack
  states, one that accepts the kind of checker the validator is, and decided by no other validator.
  It may also say which of the pack's files it `reads`, such as the policies a policy engine
  evaluates: each has to be a file the pack holds, named once. Otherwise the pack is refused with
  `bad_validator`.
- **Obligations:** IR contracts stating the policy defaults for the pack's obligations, described
  below. Optional.

Paths are relative and `/`-separated, with no `..`.

The **manifest digest**, over the manifest's canonical bytes, is the pack's identity, and what is signed.
The message signed is `capsulet.pack/v1 manifest <digest>`, so a pack signature cannot be presented
as a signature over anything else.

## What installing checks

`admit` refuses a pack for the first of these that holds, each with its code:

| Code | When |
| --- | --- |
| `unsupported_schema`, `bad_name`, `bad_version` | the manifest is not a `capsulet.pack/v1` manifest of a well-named, semantically versioned pack |
| `unsafe_path` | a listed path escapes the pack |
| `unlisted_file`, `missing_file`, `tampered_file` | the files on disk are not exactly the files listed, by digest |
| `trust_store` | a trusted key's id is not the digest of its public key |
| `unsigned` | there is no `pack.sig` |
| `signature_for_another_manifest` | the signature is over another manifest, for example one edited after signing |
| `unknown_key`, `key_not_trusted_for_pack` | the key is not in the trust store, or not trusted for this pack's name |
| `bad_signature` | the signature does not verify |
| `downgrade` | a later version of the pack is installed |
| `version_reused` | this version is installed with other content |
| `bad_contract`, `bad_validator`, `bad_cases` | a contract does not compile, or a registration or cases file does not read |
| `bad_obligations` | an obligations file is not an IR contract, or its policy cannot be met |

The same version with the same digest is the same pack. Installing it again changes nothing.

## Keys and trust

A **trust store** (`capsulet.pack-trust/v1`) holds public keys, each with the pack names it may sign,
or `*`. A key's id is the digest of its public key. A **private key** file (`capsulet.pack-key/v1`) is a
secret: it never goes in a pack or a repository.

## Commands

| Command | Does |
| --- | --- |
| `capsulet pack keygen --key <file> --trust <file> --packs <name>…` | makes a key, never overwriting one, and trusts its public half for the named packs |
| `capsulet pack refresh <dir>` | lists every file in the manifest by digest |
| `capsulet pack sign <dir> --key <file>` | refreshes, then writes `pack.sig` |
| `capsulet pack verify <dir> --trust <file>` | checks the pack as installing would, and exits non-zero when it would be refused |
| `capsulet pack test <dir>` | runs each validator against its cases, without a stack, and exits non-zero when any fails |

`packs/template` is a working pack to start from. The `verifier-isolation` gate holds it to passing
its own test, with its validator in a real container, and to installing once signed.

The reference packs are held the same way, and each has its own tests of what it answers:

| Pack | For | Tests |
| --- | --- | --- |
| [`contract-clauses`](../../packs/contract-clauses/README.md) | answers that quote contract clauses | `crates/pack/tests/contract_clauses.rs` |
| [`code-changes`](../../packs/code-changes/README.md) | whether a code change builds and its tests show it | `crates/pack/tests/code_changes.rs` |
| [`infrastructure-policy`](../../packs/infrastructure-policy/README.md) | whether an infrastructure change is permitted by policy, and stays inside its scope | `crates/pack/tests/infrastructure_policy.rs` |

## The policy defaults for a pack's obligations

`obligations` lists IR contracts by path. Each is a contract in the IR's own form, whose obligations
say which kinds of checker may discharge them (`accepts`), the authority a person needs to
discharge one by review (`authority`), and the authority a person needs to waive one
(`waiver_authority`). A definition that uses the pack declares these contracts as
they are and pins the pack, and an obligation discharged any other way is refused when its
certificate is sealed. Admission refuses an obligations file, with `bad_obligations`, when:

- it is not an IR contract;
- two files state the same contract, or a contract states an obligation twice;
- an obligation accepts only `human_review` and names no authority, so no review of it would count;
- an obligation names an authority and accepts no `human_review`, so the authority is never used.

A manifest with no `obligations` digests exactly as it did before packs carried any. The
[contract-clauses pack](../../packs/contract-clauses/README.md) is the reference: a reading of a
clause is discharged by a review under `review:legal` and by nothing else.

## Deciding a change with a pack

`capsulet_pack::decide` runs each validator that decides something over a change's inputs, and
returns what a certificate needs:

- every obligation the pack states, in the state the runs left it;
- the record of each run;
- the inputs, cited as `inputs/<path>`, and each run's outputs, as evidence with their bytes.

An obligation no validator decides stays residual, naming the authority whose review it waits for. A
validator that cannot run decides nothing, and its obligations stay residual with the failure's code.
`capsulet_pack::replay_tools` runs the tools again from the certificate's bundle, which
`capsulet pack replay` does from the command line ([verifier protocol](verifier-protocol.md)).

Each validator is also handed the pack's files it `reads`, at `pack/` and the path. They are cited
as inputs like the change's own, supplied by the pack at its version, so a certificate pins which
policy text decided it, and a replay hands each tool exactly what it read. A change's inputs may
not reach under `pack/`: `decide` refuses one that does, with `reserved_input`. `capsulet pack test`
hands each case the files its validator reads, the same way.

## Running a pack in a run

A definition that pins a pack runs its validators in a verifier node whose `capsulet.kind` is `pack`
and whose `capsulet.pack` names it ([durable run execution](durable-run-execution.md)). The node
hands the validators the files its inputs hold, and keeps from the decision the obligations the
pack's validators decide; the run's certificate, sealed from its log, holds every other one residual
unless another node of the run decides it. The `m4` gate runs all three reference packs this way, on
a stack of its own, and replays every certificate from the bundle the API serves.

## Waiving a failure

`capsulet_pack::waive` applies waivers to what a decision failed. A waiver names:

- its `id`, the obligation it waives, and the `authority` it is granted under;
- who granted it (`by`) and why (`reason`);
- the `failure` it accepts, exactly as the validator reported it.

A waiver whose failure is the obligation's makes it `waived`. The waived state names the waiver as
its policy, its authority and who granted it, and rests on the waiver itself, cited as
`waivers/<id>` with a person as its producer, so the certificate and its bundle carry it. A waived
obligation is still outstanding, so a certificate with one and nothing failed is `conditional`.

A waiver covers the failure it names and no other. One whose obligation held, or failed some other
way, is returned as having covered nothing, and the obligation keeps its state. A waiver of an
obligation the decision does not hold, of one naming no `waiver_authority`, or under another
authority than the one it names is refused, with `bad_waiver`, and nothing changes. The seal refuses
a certificate waiving an obligation under any authority but the one it names. Whether the person
granting a waiver holds its authority is for the caller to check, as the API checks a reviewer's
scopes.

## Installing a pack into a project

`POST /v1/packs` (`memory:write`, admin) takes a pack as one JSON document:

```json
{
  "manifest": { "schema_version": "capsulet.pack/v1", "name": "sample", "version": "1.0.0", "...": "..." },
  "signature": { "schema_version": "capsulet.pack-signature/v1", "key": "sha256:…", "manifest": "sha256:…", "signature": "…" },
  "files": { "contracts/sample.contract": "<the file's bytes, hex-encoded>" }
}
```

It answers `200` either way, with `newly_installed` true when the pack was installed and false when
this project already held exactly this pack. A refusal is `422 pack_refused`, with the pack's own
code in the message.

Each project's trust store is the file `<CAPSULET_PACK_TRUST_DIR>/<tenant>/<project>.json`, supplied
by the operator. It lists each key once, with every pack it is trusted for; a store listing a key
twice is refused with `trust_store`. The Compose stack mounts the host directory `CAPSULET_PACK_TRUST_HOST_DIR` there,
read-only; `stack up` and every gate's stack make one of their own. With no directory configured, or no file for the project, no signer is trusted, so
no pack installs. `GET /v1/packs` and `GET /v1/packs/{name}/{version}` (`memory:read`) report what a
project holds. An installed version is stored as it arrived and never changes, which the database
enforces.

## Pinning a pack in a definition

A definition names each pack it uses in `packs`, by `name`, `version` and `manifest` digest.
Admission refuses a pin no pack could satisfy, and refuses a pack pinned twice. Registering the
definition (`POST /v1/ir/definitions`) refuses, with `422 pack_not_installed`, a pin the project
does not hold at exactly that version and manifest digest. A certificate pins its definition's digest,
so it names the packs too.

`capsulet-replay <run-bundle.json> --packs <dir>` names each pinned pack and checks the one held at
`<dir>/<name>`: its files are the ones its manifest lists, and its manifest digest is the one pinned.
A pin the replayer does not hold, or holds as another pack, is reported and fails the replay, never
substituted. Without `--packs` the pins are named and nothing fails.
