use std::collections::BTreeSet;

use capsulet_api::{Permission, Principal, ProjectRole, Role, endpoint_contracts};

#[derive(serde::Deserialize)]
struct Case {
    operation_id: String,
    minimum_role: String,
    ownership: String,
}

#[test]
fn authorization_matrix_covers_every_public_operation_and_role_boundary() {
    let cases: Vec<Case> = serde_json::from_str(include_str!("fixtures/authorization-cases.json"))
        .expect("reviewed authorization cases");
    let expected = cases
        .iter()
        .map(|case| case.operation_id.as_str())
        .collect::<BTreeSet<_>>();
    let registered = endpoint_contracts()
        .iter()
        .map(|endpoint| endpoint.operation_id)
        .collect::<BTreeSet<_>>();
    assert_eq!(
        expected, registered,
        "a new route needs an independently reviewed authorization case"
    );
    assert_eq!(
        cases.len(),
        expected.len(),
        "duplicate cases cannot substitute for missing routes"
    );

    for case in &cases {
        let policy = capsulet_api::find_operation(&case.operation_id)
            .expect("operation")
            .policy;
        let expected_role = match case.minimum_role.as_str() {
            "anonymous" | "authenticated" => None,
            "viewer" => Some(ProjectRole::Viewer),
            "operator" => Some(ProjectRole::Operator),
            "admin" => Some(ProjectRole::Admin),
            other => panic!("unknown expected role: {other}"),
        };
        assert_eq!(
            policy.permission.minimum_project_role(),
            expected_role,
            "{}",
            case.operation_id
        );
        assert_eq!(
            policy.ownership.as_str(),
            case.ownership,
            "{}",
            case.operation_id
        );
    }
}

#[test]
fn authorization_service_account_admin_has_no_platform_privileges() {
    let principal = Principal::service_account(
        "project-ci",
        Role::Admin,
        "tenant",
        "finance",
        ["jobs:run".to_string()],
    );
    assert!(!principal.platform_admin);
    assert!(!principal.has_scope("system:write"));
}

#[test]
fn authorization_role_boundaries_follow_the_documented_job_and_memory_model() {
    assert!(!ProjectRole::Viewer.allows(Permission::JobsRun));
    assert!(ProjectRole::Operator.allows(Permission::JobsRun));
    assert!(!ProjectRole::Operator.allows(Permission::MemoryWrite));
    assert!(ProjectRole::Admin.allows(Permission::MemoryWrite));
}
