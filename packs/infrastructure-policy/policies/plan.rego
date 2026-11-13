# What the change's Terraform plan writes.
#
# The plan is `terraform show -json` of a saved plan, loaded at `data.change`.
# A policy reads `written`: every managed resource the change creates or
# updates, a replacement included, as it will be after the change. A resource
# the change deletes or only reads is not written, and the scope check, not a
# policy, decides whether the change may touch it.
package capsulet.plan

written contains resource if {
	some resource in object.get(data.change, "resource_changes", [])
	resource.mode == "managed"
	some action in resource.change.actions
	action in {"create", "update"}
}

# A list, or an empty one in place of a value that is missing or null.
list(value) := value if is_array(value)

else := []
