# no-open-admin-ports: no firewall rule the change creates or updates opens SSH
# or RDP to the whole internet.
#
# Reads `aws_security_group_rule` ingress rules and the `ingress` blocks of
# `aws_security_group`. A rule opens a port when its protocol is TCP and its
# range holds the port, or its protocol is every protocol.
package capsulet.policies.no_open_admin_ports

import data.capsulet.plan

admin_ports := {22: "SSH", 3389: "RDP"}

everywhere := {"0.0.0.0/0", "::/0"}

violations contains message if {
	some resource in plan.written
	resource.type == "aws_security_group_rule"
	resource.change.after.type == "ingress"
	some message in opened(resource.address, resource.change.after)
}

violations contains message if {
	some resource in plan.written
	resource.type == "aws_security_group"
	some rule in plan.list(object.get(resource.change.after, "ingress", null))
	some message in opened(resource.address, rule)
}

opened(address, rule) := {sprintf("%s opens %s (port %d) to %s", [address, name, port, cidr]) |
	some port, name in admin_ports
	covers(rule, port)
	some cidr in sources(rule)
	cidr in everywhere
}

sources(rule) := array.concat(
	plan.list(object.get(rule, "cidr_blocks", null)),
	plan.list(object.get(rule, "ipv6_cidr_blocks", null)),
)

covers(rule, _) if rule.protocol in {"-1", "all"}

covers(rule, port) if {
	rule.protocol in {"tcp", "6"}
	rule.from_port <= port
	port <= rule.to_port
}
