# storage-encrypted: every volume and database the change creates or updates is
# encrypted at rest.
#
# Reads `aws_ebs_volume`, `aws_db_instance` and `aws_rds_cluster`, each for the
# setting that encrypts it.
package capsulet.policies.storage_encrypted

import data.capsulet.plan

setting := {
	"aws_ebs_volume": "encrypted",
	"aws_db_instance": "storage_encrypted",
	"aws_rds_cluster": "storage_encrypted",
}

violations contains sprintf("%s is not encrypted at rest: %s is not true", [resource.address, name]) if {
	some resource in plan.written
	name := setting[resource.type]
	not resource.change.after[name] == true
}
