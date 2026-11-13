//! Projects, service accounts and the audit log.

use super::*;

#[tokio::test]
async fn audit_events_with_unverified_scope_are_retained_but_not_assigned_to_default_project() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let request_id = unique_id("unverified_audit");
    store
        .record_audit_event(
            "legacy-fixture",
            "admin",
            "POST",
            "/v1/legacy-operation",
            201,
            Some(&request_id),
            None,
        )
        .await
        .expect("legacy event");
    let scoped = store
        .list_audit_events_in_project("default", "default", 500)
        .await
        .expect("project list");
    assert!(
        !scoped
            .iter()
            .any(|event| event.request_id.as_deref() == Some(&request_id))
    );
    let retained = store
        .list_audit_events(500)
        .await
        .expect("trusted raw list");
    assert!(
        retained
            .iter()
            .any(|event| event.request_id.as_deref() == Some(&request_id))
    );
}

#[tokio::test]
async fn audit_listing_excludes_foreign_projects_and_tenants_before_limit() {
    use capsulet_postgres::{AuditScope, NewAuditEvent};
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let project = unique_id("audit_project");
    let own = NewAuditEvent {
        principal: "audit-fixture",
        role: "operator",
        method: "POST",
        path: "/v1/owned-operation",
        status_code: 201,
        request_id: None,
        user_agent: None,
        scope: Some(AuditScope {
            tenant_id: "default",
            project_id: &project,
        }),
    };
    store
        .record_scoped_audit_event(&own)
        .await
        .expect("owned event");
    store
        .record_scoped_audit_event(&NewAuditEvent {
            path: "/v1/foreign-operation",
            scope: Some(AuditScope {
                tenant_id: "default",
                project_id: "foreign-audit-project",
            }),
            ..own
        })
        .await
        .expect("newer foreign event");
    let visible = store
        .list_audit_events_in_project("default", &project, 1)
        .await
        .expect("list");
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].path, "/v1/owned-operation");
    assert_eq!(
        store
            .list_audit_events_in_project("foreign-tenant", &project, 10)
            .await
            .expect("list"),
        [] as [capsulet_postgres::AuditEvent; 0]
    );
}

/// A project id belongs to one tenant: its own tenant can rename it, any other
/// tenant is refused without changing it, and a missing tenant gets nothing.
#[tokio::test]
async fn a_project_id_belongs_to_one_tenant() {
    let store = scoped_store().await;
    let id = unique_id("provisioned");
    let other_tenant = unique_id("other_tenant");
    sqlx::query("INSERT INTO tenants (id, name) VALUES ($1, 'Other tenant')")
        .bind(&other_tenant)
        .execute(store.pool())
        .await
        .expect("tenant");
    let project = |tenant_id: &str, name: &str| capsulet_postgres::ProjectRecord {
        id: id.clone(),
        tenant_id: tenant_id.to_string(),
        name: name.to_string(),
    };

    let created = store
        .upsert_project(&project("default", "Provisioned"))
        .await
        .expect("create");
    assert_eq!(created, Some(project("default", "Provisioned")));
    let repeated = store
        .upsert_project(&project("default", "Provisioned"))
        .await
        .expect("repeat");
    assert_eq!(repeated, created, "the same request changes nothing");
    let renamed = store
        .upsert_project(&project("default", "Renamed"))
        .await
        .expect("rename");
    assert_eq!(renamed, Some(project("default", "Renamed")));
    let taken = store
        .upsert_project(&project(&other_tenant, "Taken"))
        .await
        .expect("take");
    assert_eq!(taken, None, "another tenant cannot take the id");
    let missing = capsulet_postgres::ProjectRecord {
        id: unique_id("orphan"),
        tenant_id: unique_id("no_such_tenant"),
        name: "Orphan".to_string(),
    };
    assert_eq!(store.upsert_project(&missing).await.expect("orphan"), None);

    assert_eq!(
        store
            .list_all_projects("default")
            .await
            .expect("list")
            .into_iter()
            .filter(|listed| listed.id == id)
            .collect::<Vec<_>>(),
        [project("default", "Renamed")]
    );
    assert_eq!(
        store.list_all_projects(&other_tenant).await.expect("list"),
        [] as [capsulet_postgres::ProjectRecord; 0]
    );
}

#[tokio::test]
async fn service_accounts_are_listed_and_revoked_only_in_their_own_project() {
    let store = PostgresStore::connect(&database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate");
    let project_id = unique_id("finance");
    sqlx::query(
        "INSERT INTO projects (id, tenant_id, name) VALUES ($1, 'default', 'Finance fixture')",
    )
    .bind(&project_id)
    .execute(store.pool())
    .await
    .expect("create project fixture");
    let account_id = unique_id("isolated_account");
    // Derived from the id, because token hashes are unique and a fixed one collides
    // with the last run's row in any database that outlives the run.
    let mut token_hash = [0_u8; 32];
    for (slot, byte) in token_hash.iter_mut().zip(account_id.bytes().rev()) {
        *slot = byte;
    }
    let account = capsulet_postgres::NewServiceAccount {
        id: account_id,
        name: "project-ci".to_string(),
        tenant_id: "default".to_string(),
        project_id,
        role: "admin".to_string(),
        scopes: vec!["jobs:read".to_string()],
        token_hash,
        expires_at_unix: None,
    };
    let saved = store
        .create_service_account(&account)
        .await
        .expect("create");
    assert_eq!(
        store
            .list_service_accounts(&account.tenant_id, &account.project_id, 10)
            .await
            .expect("list"),
        vec![saved]
    );
    assert_eq!(
        store
            .list_service_accounts("other-tenant", &account.project_id, 10)
            .await
            .expect("list"),
        [] as [capsulet_postgres::ServiceAccountRecord; 0]
    );
    assert_eq!(
        store
            .list_service_accounts(&account.tenant_id, "other-project", 10)
            .await
            .expect("list"),
        [] as [capsulet_postgres::ServiceAccountRecord; 0]
    );
    assert!(
        !store
            .revoke_service_account(&account.tenant_id, "other-project", &account.id)
            .await
            .expect("revoke")
    );
    assert!(
        !store
            .revoke_service_account("other-tenant", &account.project_id, &account.id)
            .await
            .expect("revoke")
    );
    assert!(
        store
            .authenticate_service_account_hash(&account.token_hash)
            .await
            .expect("authenticate")
            .is_some()
    );
    assert!(
        store
            .revoke_service_account(&account.tenant_id, &account.project_id, &account.id)
            .await
            .expect("revoke")
    );
    assert!(
        store
            .authenticate_service_account_hash(&account.token_hash)
            .await
            .expect("authenticate")
            .is_none()
    );
}
