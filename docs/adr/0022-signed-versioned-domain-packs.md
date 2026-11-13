# ADR 0022: Signed, Versioned Domain Packs

Status: Accepted

Tracked in the [M4 milestone](https://github.com/mohripan/capsulet/milestone/7); this decision is
part of #158.

## Context

[ADR 0021 §5](0021-the-verifier-protocol-boundary.md#5-domain-packs-carry-everything-domain-specific)
puts everything domain-specific in a **domain pack**:

- contracts, relations, phrasings and cues;
- qualifier and reference declarations;
- validator registrations with their bases and capabilities;
- the cases that show the validators work.

The kernel and runtime contain none of it, and M4's gate is three unrelated packs on one protocol.

A pack decides what a certificate checks. A contract's phrasings decide what a citation states, its
qualifier cues decide what a clause leaves out, and a validator's registration decides whether its
`holds` discharges anything. So installing a pack is a trust decision. A run decided under a pack has
to be replayable under exactly that pack, years later and offline.

Capsulet already signs what it ships: images and the chart through Sigstore, keyless, with the
release workflow's identity, verified against the transparency log
([supply chain](../security/supply-chain.md)). That fits artifacts Capsulet builds itself. Packs are
written by domain experts outside the project, installed per project, and verified at install and at
replay, often without network.

## Decision

### 1. A pack is a directory with a manifest

`pack.json`, under the schema `capsulet.pack/v1`, names:

- the pack, and its semantic version;
- every file in the pack, by path and SHA-256 digest. A file the manifest does not list makes the
  pack unreadable, and so does a listed file whose bytes differ;
- the contracts, as memory contract DSL files;
- the validators, each a registration (`capsulet.verifier/v1`) with its cases file.

The **manifest digest**, over the manifest's canonical bytes, is the pack's identity. Name and version
say what a pack claims to be, and the digest says what it is.

### 2. A pack is signed with Ed25519, against keys a project trusts

`pack.sig` is an Ed25519 signature over the manifest digest. It names the key it was made with, by
the digest of the public key. A project trusts publishers through a **trust store**: public keys, each
with the pack names it may sign (`*` for any). A pack installs only when:

- it carries a signature;
- the signature is over this manifest's digest;
- the key is in the trust store, and trusted for this pack's name;
- the signature verifies.

This works offline and the same way every time, with nothing to fetch at install or at replay, and
it needs no new dependency: verification is `aws-lc-rs`, which the API already builds. Sigstore keyless
stays the way Capsulet's own images and chart are signed. A pack the project publishes itself may be
signed both ways. Installing it reads only the Ed25519 signature, because installation must not
depend on reaching a transparency log.

A key is rotated by trusting the new one before packs are signed with it, and revoked by removing it.
Revocation does not rewrite history: a certificate decided under a pack keeps naming that pack's
manifest digest, and replay re-reads the pack it pinned.

### 3. A version is immutable and does not go backwards

For each pack name, a project holds the versions it installed:

- the same version with the same manifest digest is the same pack, and installing it again changes
  nothing;
- the same version with another digest is refused: a version is a promise about content;
- a version lower than the highest installed is refused as a downgrade. Going back needs a new
  version with the old content, so the history says so.

### 4. A run pins the pack, and replay uses what it pinned

A definition names each pack it uses by name, version and manifest digest. A certificate decided
under it carries that pin, so it says exactly which pack decided it, and a replay re-reads that pack
and no other. A pin that names a pack the replayer does not hold, or holds with another digest, is
reported, never substituted.

### 5. A pack is tested before it is signed

`capsulet pack test <dir>` runs each validator against its cases through the verifier harness,
without a stack. `capsulet pack keygen`, `sign` and `verify` produce keys, sign a pack, and check one
against a trust store as installation would. `packs/template` is a working pack to start from, and a
gate holds it to passing its own test.

## Consequences

- A certificate names the exact pack that decided it, and an installed pack cannot change under it.
- A pack is only as trustworthy as the key that signed it, and the project decides which keys those
  are, per pack name. There is no ambient trust, including in packs Capsulet ships.
- Because the trust store is per project, two projects can trust different publishers for the same
  pack name without interfering.

## Not decided here

- Where the trust store lives in a deployment, and how its changes are audited. It follows the
  project IAM work, and until then it is a file the operator supplies.
- Distributing packs from a registry. A pack is a directory, and how it is fetched does not change
  what is verified.
