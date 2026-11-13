# Domain Packs

Status: experimental. A domain pack carries everything domain-specific, as
[ADR 0022](../adr/0022-signed-versioned-domain-packs.md) decided it and `crates/pack` implements it:
contracts, and validators with the cases that show they work. The kernel and runtime contain none of
it.

## The directory

| File | Holds |
| --- | --- |
| `pack.json` | the manifest, `capsulet.pack/v1`: `name`, `version`, `description`, `contracts`, `validators` and `files` |
| `pack.sig` | the signature, `capsulet.pack-signature/v1`: the key, the manifest digest, and the Ed25519 signature over it |
| anything else | listed in `files` by path and SHA-256 digest |

- **Name:** lower-case letters, digits and hyphens, starting with a letter.
- **Version:** semantic.
- **Contracts:** memory contract DSL files, which must parse and compile.
- **Validators:** each a `capsulet.verifier/v1` registration with its cases file
  ([verifier protocol](verifier-protocol.md)). The registration must be one the host may run.

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
by the operator. With no directory configured, or no file for the project, no signer is trusted, so
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
