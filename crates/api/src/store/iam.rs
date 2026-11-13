//! Projects, memberships, service accounts and the audit log.

use async_trait::async_trait;
use capsulet_postgres::{
    AuditEvent, NewAuditEvent, NewProjectMembership, NewServiceAccount, PostgresStore,
    ProjectMembershipRecord, ProjectRecord, ServiceAccountRecord,
};

use super::StoreBase;

/// Projects, memberships, service accounts and the audit log.
#[async_trait]
pub trait IamStore: StoreBase {
    async fn list_audit_events(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<AuditEvent>, Self::Error> {
        Ok(Vec::new())
    }

    async fn list_projects(
        &self,
        _tenant_id: &str,
        _project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn list_all_projects(&self, _tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(Vec::new())
    }

    /// Creates the project in its tenant or renames it there. `None` means the
    /// id is not available to that tenant.
    async fn upsert_project(
        &self,
        _project: &ProjectRecord,
    ) -> Result<Option<ProjectRecord>, Self::Error> {
        unreachable!("project provisioning is unsupported by this store")
    }

    async fn list_project_memberships(
        &self,
        _tenant_id: &str,
        _project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn list_principal_project_memberships(
        &self,
        _tenant_id: &str,
        _principal_name: &str,
        _principal_kind: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn upsert_project_membership(
        &self,
        _membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        unreachable!("project membership creation is unsupported by this store")
    }

    async fn delete_project_membership(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _principal_kind: &str,
        _principal_name: &str,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    async fn resource_project(
        &self,
        _resource: &str,
        _id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        Ok(None)
    }

    async fn authenticate_service_account_hash(
        &self,
        _token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, Self::Error> {
        Ok(None)
    }

    async fn create_service_account(
        &self,
        _account: &NewServiceAccount,
    ) -> Result<ServiceAccountRecord, Self::Error> {
        unreachable!("service account creation is unsupported by this store")
    }

    async fn list_service_accounts(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<ServiceAccountRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn revoke_service_account(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _id: &str,
    ) -> Result<bool, Self::Error> {
        Ok(false)
    }

    async fn record_audit_event(&self, _event: &NewAuditEvent<'_>) -> Result<(), Self::Error> {
        Ok(())
    }
}

#[async_trait]
impl IamStore for PostgresStore {
    async fn list_audit_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<AuditEvent>, Self::Error> {
        self.list_audit_events_in_project(tenant_id, project_id, limit)
            .await
    }

    async fn list_projects(
        &self,
        tenant_id: &str,
        project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        PostgresStore::list_projects(self, tenant_id, project_ids).await
    }

    async fn list_all_projects(&self, tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        PostgresStore::list_all_projects(self, tenant_id).await
    }

    async fn upsert_project(
        &self,
        project: &ProjectRecord,
    ) -> Result<Option<ProjectRecord>, Self::Error> {
        PostgresStore::upsert_project(self, project).await
    }

    async fn list_project_memberships(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        PostgresStore::list_project_memberships(self, tenant_id, project_id).await
    }

    async fn list_principal_project_memberships(
        &self,
        tenant_id: &str,
        principal_name: &str,
        principal_kind: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        PostgresStore::list_principal_project_memberships(
            self,
            tenant_id,
            principal_name,
            principal_kind,
        )
        .await
    }

    async fn upsert_project_membership(
        &self,
        membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        PostgresStore::upsert_project_membership(self, membership).await
    }

    async fn delete_project_membership(
        &self,
        tenant_id: &str,
        project_id: &str,
        principal_kind: &str,
        principal_name: &str,
    ) -> Result<bool, Self::Error> {
        PostgresStore::delete_project_membership(
            self,
            tenant_id,
            project_id,
            principal_kind,
            principal_name,
        )
        .await
    }

    async fn resource_project(
        &self,
        resource: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        PostgresStore::resource_project(self, resource, id).await
    }

    async fn authenticate_service_account_hash(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, Self::Error> {
        PostgresStore::authenticate_service_account_hash(self, token_hash).await
    }

    async fn create_service_account(
        &self,
        account: &NewServiceAccount,
    ) -> Result<ServiceAccountRecord, Self::Error> {
        PostgresStore::create_service_account(self, account).await
    }

    async fn list_service_accounts(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<ServiceAccountRecord>, Self::Error> {
        PostgresStore::list_service_accounts(self, tenant_id, project_id, limit).await
    }

    async fn revoke_service_account(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<bool, Self::Error> {
        PostgresStore::revoke_service_account(self, tenant_id, project_id, id).await
    }

    async fn record_audit_event(&self, event: &NewAuditEvent<'_>) -> Result<(), Self::Error> {
        self.record_scoped_audit_event(event).await
    }
}
