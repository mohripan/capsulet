//! Projects, memberships, service accounts and the audit log.

use super::super::*;

#[async_trait::async_trait]
impl IamStore for FakeStore {
    async fn record_audit_event(
        &self,
        event: &capsulet_postgres::NewAuditEvent<'_>,
    ) -> Result<(), Self::Error> {
        let mut events = self
            .audit_events
            .lock()
            .map_err(|error| error.to_string())?;
        let id = i64::try_from(events.len() + 1).map_err(|error| error.to_string())?;
        events.push(FakeAuditEvent {
            event: capsulet_postgres::AuditEvent {
                id,
                principal: event.principal.to_string(),
                role: event.role.to_string(),
                method: event.method.to_string(),
                path: event.path.to_string(),
                status_code: i32::from(event.status_code),
                request_id: event.request_id.map(str::to_owned),
                created_at: "2026-10-11T00:00:00Z".to_string(),
            },
            scope: event
                .scope
                .map(|scope| (scope.tenant_id.to_string(), scope.project_id.to_string())),
        });
        Ok(())
    }

    async fn list_audit_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<capsulet_postgres::AuditEvent>, Self::Error> {
        let limit = usize::try_from(limit.clamp(1, 500)).map_err(|error| error.to_string())?;
        Ok(self
            .audit_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .rev()
            .filter(|record| {
                record
                    .scope
                    .as_ref()
                    .is_some_and(|(tenant, project)| tenant == tenant_id && project == project_id)
            })
            .take(limit)
            .map(|record| record.event.clone())
            .collect())
    }

    async fn authenticate_service_account_hash(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<ServiceAccountRecord>, Self::Error> {
        Ok(self
            .service_accounts
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|account| {
                &account.token_hash == token_hash && account.record.revoked_at.is_none()
            })
            .map(|account| account.record.clone()))
    }

    async fn list_projects(
        &self,
        tenant_id: &str,
        project_ids: &[String],
    ) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(self
            .projects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|project| {
                project.tenant_id == tenant_id && project_ids.iter().any(|id| id == &project.id)
            })
            .cloned()
            .collect())
    }

    async fn list_all_projects(&self, tenant_id: &str) -> Result<Vec<ProjectRecord>, Self::Error> {
        Ok(self
            .projects
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|project| project.tenant_id == tenant_id)
            .cloned()
            .collect())
    }

    async fn upsert_project(
        &self,
        project: &ProjectRecord,
    ) -> Result<Option<ProjectRecord>, Self::Error> {
        let mut projects = self.projects.lock().map_err(|error| error.to_string())?;
        match projects
            .iter_mut()
            .find(|existing| existing.id == project.id)
        {
            Some(existing) if existing.tenant_id != project.tenant_id => Ok(None),
            Some(existing) => {
                existing.name.clone_from(&project.name);
                Ok(Some(existing.clone()))
            }
            None => {
                projects.push(project.clone());
                Ok(Some(project.clone()))
            }
        }
    }

    async fn list_project_memberships(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|membership| {
                membership.tenant_id == tenant_id && membership.project_id == project_id
            })
            .cloned()
            .collect())
    }

    async fn list_principal_project_memberships(
        &self,
        tenant_id: &str,
        principal_name: &str,
        principal_kind: &str,
    ) -> Result<Vec<ProjectMembershipRecord>, Self::Error> {
        Ok(self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|membership| {
                membership.tenant_id == tenant_id
                    && membership.principal_name == principal_name
                    && membership.principal_kind == principal_kind
            })
            .cloned()
            .collect())
    }

    async fn upsert_project_membership(
        &self,
        membership: &NewProjectMembership,
    ) -> Result<ProjectMembershipRecord, Self::Error> {
        let mut records = self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?;
        records.retain(|record| {
            !(record.tenant_id == membership.tenant_id
                && record.project_id == membership.project_id
                && record.principal_kind == membership.principal_kind
                && record.principal_name == membership.principal_name)
        });
        let record = ProjectMembershipRecord {
            id: membership.id.clone(),
            tenant_id: membership.tenant_id.clone(),
            project_id: membership.project_id.clone(),
            principal_kind: membership.principal_kind.clone(),
            principal_name: membership.principal_name.clone(),
            role: membership.role.clone(),
            created_by: membership.created_by.clone(),
            created_at: "2026-06-24 00:00:00+00".to_string(),
            updated_at: "2026-06-24 00:00:00+00".to_string(),
        };
        records.push(record.clone());
        Ok(record)
    }

    async fn delete_project_membership(
        &self,
        tenant_id: &str,
        project_id: &str,
        principal_kind: &str,
        principal_name: &str,
    ) -> Result<bool, Self::Error> {
        let mut records = self
            .project_memberships
            .lock()
            .map_err(|error| error.to_string())?;
        let initial_len = records.len();
        records.retain(|record| {
            !(record.tenant_id == tenant_id
                && record.project_id == project_id
                && record.principal_kind == principal_kind
                && record.principal_name == principal_name)
        });
        Ok(records.len() != initial_len)
    }

    async fn resource_project(
        &self,
        resource: &str,
        id: &str,
    ) -> Result<Option<(String, String)>, Self::Error> {
        if self.omit_ownership {
            return Ok(None);
        }
        Ok(self
            .ownership
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|ownership| ownership.resource == resource && ownership.id == id)
            .map(|ownership| (ownership.tenant_id.clone(), ownership.project_id.clone()))
            .or_else(|| Some(("default".to_string(), "default".to_string()))))
    }
}
