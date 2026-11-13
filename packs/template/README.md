# Pack template

A working domain pack to start from ([ADR 0022](../../docs/adr/0022-signed-versioned-domain-packs.md)).
Copy the directory, rename it, and replace its contents with your domain's own.

| Path | Holds |
| --- | --- |
| `pack.json` | the name, the version, and what the pack contains |
| `contracts/template.contract` | a memory contract: entities, relations, the phrasings that state them, and how the documents it reads are structured |
| `validators/inputs-not-empty.json` | a validator registration (`capsulet.verifier/v1`): a replayable tool in a busybox image pinned by digest |
| `cases/inputs-not-empty.json` | the cases that show the validator works |

The validator is a few lines of shell so the template needs nothing but Docker. A real one is an image
built from a validator written with the Rust or Python SDK
([verifier protocol](../../docs/contracts/verifier-protocol.md)).

```sh
capsulet pack test packs/template
capsulet pack keygen --key publisher.key.json --trust trust.json --packs template
capsulet pack sign packs/template --key publisher.key.json
capsulet pack verify packs/template --trust trust.json
```

- `pack test` runs each validator against its cases, without a stack.
- `pack keygen` makes a publisher key, and a trust store holding its public half for the named packs.
  The key is a secret: keep it out of the pack and out of the repository.
- `pack sign` lists every file in `pack.json` by digest, then signs the manifest.
- `pack verify` checks the pack the way installing it would: every file is listed and unchanged, the
  signature verifies under a key trusted for this pack, and the contracts and validators read.

The template is committed unsigned, so `pack verify` reports it `unsigned` until you sign it.
