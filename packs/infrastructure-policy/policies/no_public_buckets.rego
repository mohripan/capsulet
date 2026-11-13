# no-public-buckets: no storage bucket the change creates or updates can be
# read by the public.
#
# Reads `aws_s3_bucket` and `aws_s3_bucket_acl` for a public canned ACL, and
# `aws_s3_bucket_public_access_block` for a block left off.
package capsulet.policies.no_public_buckets

import data.capsulet.plan

public_acls := {"public-read", "public-read-write"}

blocks := ["block_public_acls", "block_public_policy", "ignore_public_acls", "restrict_public_buckets"]

violations contains sprintf("%s grants %s", [resource.address, resource.change.after.acl]) if {
	some resource in plan.written
	resource.type in {"aws_s3_bucket", "aws_s3_bucket_acl"}
	resource.change.after.acl in public_acls
}

violations contains sprintf("%s leaves %s off", [resource.address, block]) if {
	some resource in plan.written
	resource.type == "aws_s3_bucket_public_access_block"
	some block in blocks
	not resource.change.after[block] == true
}
