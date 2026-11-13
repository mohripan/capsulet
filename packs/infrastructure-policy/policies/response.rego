# The capsulet.verifier/v1 response to the request on standard input.
#
# OPA evaluates `data.capsulet.response` with the request as `input` and the
# change's Terraform plan at `data.change`. Each obligation asked for is decided
# by the policy of the same name, the package under `data.capsulet.policies`
# whose name is the obligation's with underscores for hyphens. A policy states
# its `violations`, and holds when it has none.
package capsulet

response := {
	"protocol": "capsulet.verifier/v1",
	"results": [result(obligation) | some obligation in input.obligations],
}

result(obligation) := object.union(
	{"obligation": obligation.id, "contract": obligation.contract},
	decision(obligation.id),
)

policy(id) := data.capsulet.policies[replace(id, "-", "_")]

decision(id) := {
	"outcome": "cannot_decide",
	"reason": "change/plan.json is not a Terraform plan: it has no format_version",
} if {
	not is_string(object.get(data.change, "format_version", null))
} else := {
	"outcome": "cannot_decide",
	"reason": sprintf("no policy in the pack is named %s", [id]),
} if {
	not policy(id)
} else := {
	"outcome": "holds",
	"reason": sprintf("nothing the change creates or updates breaks it (%s checked)", [counted(count(data.capsulet.plan.written))]),
} if {
	count(policy(id).violations) == 0
} else := {
	"outcome": "does_not_hold",
	"reason": sprintf("%s: %s", [id, concat("; ", sort(policy(id).violations))]),
}

counted(1) := "1 resource"

counted(n) := sprintf("%d resources", [n]) if n != 1
