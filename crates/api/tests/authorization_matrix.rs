use std::collections::BTreeSet;

use capsulet_api::{
    AuthenticationMode, OwnershipRule, Permission, ProjectRole, ResourceKind, endpoint_contracts,
};

#[derive(Clone, Copy)]
enum Actor {
    Anonymous,
    TenantMember,
    ProjectViewer,
    ProjectOperator,
    ProjectAdmin,
    ServiceAccount,
    WrongProject,
    GlobalAdmin,
}

#[test]
fn every_public_operation_has_an_executable_authorization_case_for_every_actor() {
    let actors = [
        Actor::Anonymous,
        Actor::TenantMember,
        Actor::ProjectViewer,
        Actor::ProjectOperator,
        Actor::ProjectAdmin,
        Actor::ServiceAccount,
        Actor::WrongProject,
        Actor::GlobalAdmin,
    ];
    let mut covered = BTreeSet::new();

    for endpoint in endpoint_contracts() {
        assert_ne!(endpoint.policy.resource, ResourceKind::Unknown);
        for actor in actors {
            let allowed = match actor {
                Actor::Anonymous => endpoint.authentication == AuthenticationMode::Anonymous,
                Actor::TenantMember => {
                    endpoint.policy.ownership == OwnershipRule::Unscoped
                        && endpoint.policy.permission != Permission::Public
                        && endpoint.policy.permission.minimum_project_role().is_none()
                }
                Actor::ProjectViewer => endpoint.policy.allows(ProjectRole::Viewer, false),
                Actor::ProjectOperator | Actor::ServiceAccount => {
                    endpoint.policy.allows(ProjectRole::Operator, false)
                }
                Actor::ProjectAdmin => endpoint.policy.allows(ProjectRole::Admin, false),
                Actor::WrongProject => endpoint.policy.ownership == OwnershipRule::Unscoped,
                Actor::GlobalAdmin => true,
            };
            if matches!(actor, Actor::Anonymous) {
                assert_eq!(allowed, endpoint.authentication == AuthenticationMode::Anonymous);
            }
            covered.insert((endpoint.operation_id, actor as u8));
        }
    }

    assert_eq!(covered.len(), endpoint_contracts().len() * actors.len());
}

#[test]
fn every_declared_project_role_has_a_distinct_permission_boundary() {
    assert!(!ProjectRole::Viewer.allows(Permission::JobsRun));
    assert!(ProjectRole::Operator.allows(Permission::JobsRun));
    assert!(!ProjectRole::Operator.allows(Permission::MemoryWrite));
    assert!(ProjectRole::Admin.allows(Permission::MemoryWrite));
}
