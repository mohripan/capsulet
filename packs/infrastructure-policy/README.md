# Infrastructure policy

A reference domain pack ([ADR 0021](../../docs/adr/0021-the-verifier-protocol-boundary.md),
[ADR 0022](../../docs/adr/0022-signed-versioned-domain-packs.md)) for one question: is a proposed
infrastructure change permitted by policy? A policy engine and a scanner decide it, each a replayable
tool in a container pinned by digest. A person may waive a policy's failure under a named authority,
and the certificate records that as a waiver, never as a pass.

| Path | Holds |
| --- | --- |
| `obligations/infrastructure-policy.json` | the obligations, what may discharge each, and who may waive it |
| `policies/` | the policies, in Rego, which the policy engine evaluates |
| `validators/opa-policies.json` | the policy engine: OPA 1.21.0, `openpolicyagent/opa@sha256:9eb36ac3…` |
| `validators/plan-scope.json` | the scope scanner: a script in `python@sha256:9ef1a599…` |
| `validators/src/` | the scanner's script, as its registration runs it |
| `cases/` | the cases that show each validator works |
| `fixtures/` | five plans written for this pack, each with the scope its request declared |

## The obligations

| Obligation | Decided by | If it does not hold | Waivable by |
| --- | --- | --- | --- |
| `no-public-buckets` | `opa-policies`: no S3 bucket the change creates or updates grants a public ACL or leaves a public access block off | `rejected`, owned by the proposer | `review:security` |
| `storage-encrypted` | `opa-policies`: every EBS volume, RDS instance and RDS cluster it creates or updates is encrypted at rest | `rejected`, owned by the proposer | `review:security` |
| `no-open-admin-ports` | `opa-policies`: no security group rule it creates or updates opens SSH (22) or RDP (3389) to `0.0.0.0/0` or `::/0` | `rejected`, owned by the proposer | `review:security` |
| `within-declared-scope` | `plan-scope`: every resource it touches is one the scope names, under an action the scope allows | `rejected`, owned by the proposer | nobody |

Every obligation accepts only `replayable_tool`. A policy's failure names the rule and each resource
that breaks it, for example `no-public-buckets: aws_s3_bucket_acl.docs grants public-read`. A change
outside its scope cannot be waived: fix the change, or change the scope its request declares.

## The inputs

- `change/plan.json`: the change, as `terraform show -json` of a saved plan.
- `scope.json`: the scope the change's request declares. Each entry is an address and the actions it
  allows, out of `create`, `update`, `delete`, `replace` and `forget`. An address ending in `*` names
  every address that starts with what comes before it.

```json
{
  "resources": [
    { "address": "aws_s3_bucket.logs", "actions": ["create"] },
    { "address": "aws_security_group_rule.*", "actions": ["create", "update"] }
  ]
}
```

The policies are not inputs a change can bring. The policy engine `reads` them from the pack, which
hands them over at `pack/policies/`, and a change's inputs may not reach under `pack/`. They are
cited in the certificate like the change's own inputs, so it pins which policy text decided.

## Policies

Each obligation is decided by the policy of the same name, the Rego package
`capsulet.policies.<name>`, with underscores for hyphens. A policy states its `violations`, and holds
when it has none. `policies/plan.rego` gives every policy `written`: each managed resource the plan
creates or updates, as it will be after. `policies/response.rego` turns the policies into the
`capsulet.verifier/v1` response, so OPA answers the protocol itself:

```text
opa eval --stdin-input --format raw --data /inputs/pack/policies \
  --data change:/inputs/change/plan.json data.capsulet.response
```

To add a policy, add its obligation to `obligations/infrastructure-policy.json`, its package to
`policies/`, the obligation to what `opa-policies` `decides`, and the file to what it `reads`.

## Waivers

A waiver names the obligation it waives, the authority it is granted under, who granted it and why,
and the failure it accepts, exactly as the policy reported it:

```json
{
  "id": "docs-site-is-public",
  "obligation": "no-public-buckets",
  "authority": "review:security",
  "by": "dana",
  "reason": "The documentation site is public by design.",
  "failure": "no-public-buckets: aws_s3_bucket_acl.docs grants public-read"
}
```

`capsulet_pack::waive` makes that failure `waived`, naming the waiver, its authority and who granted
it, and cites the waiver as evidence. A certificate with a waived obligation and nothing failed is
`conditional`. A waiver covers the failure it names and no other: a change that also opens another
bucket fails with a failure the waiver does not name, and stays `rejected`. A waiver under any
authority but `review:security` is refused, and so is one of `within-declared-scope`.

## What a check leaves behind

Both validators run with no network, a read-only root and inputs, and only `/outputs` writable. The
scanner writes `scope.log`, an inventory of every resource the plan touches and whether it is in
scope. OPA writes nothing beyond its response, so a policy's failure is in the obligation's reason.
Its record pins the plan and every policy it read by digest.

## Using it

```sh
capsulet pack test packs/infrastructure-policy
capsulet pack replay certificate-bundle.json packs/infrastructure-policy
capsulet pack sign packs/infrastructure-policy --key publisher.key.json
```

Testing and replaying need Docker. The pack is committed unsigned, like the other reference packs.
