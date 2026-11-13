//! The domain packs a project holds.

use async_trait::async_trait;
use capsulet_postgres::{DomainPackRecord, PostgresStore};

use super::StoreBase;

/// The domain packs a project holds.
#[async_trait]
pub trait PackStore: StoreBase {
    /// Records an installed pack version; `false` when that version is already held.
    async fn insert_domain_pack(&self, record: &DomainPackRecord) -> Result<bool, Self::Error>;

    async fn list_domain_packs(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<DomainPackRecord>, Self::Error>;

    async fn find_domain_pack(
        &self,
        tenant_id: &str,
        project_id: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<DomainPackRecord>, Self::Error>;
}

#[async_trait]
impl PackStore for PostgresStore {
    async fn insert_domain_pack(&self, record: &DomainPackRecord) -> Result<bool, Self::Error> {
        self.insert_domain_pack(record).await
    }

    async fn list_domain_packs(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<DomainPackRecord>, Self::Error> {
        self.list_domain_packs(tenant_id, project_id).await
    }

    async fn find_domain_pack(
        &self,
        tenant_id: &str,
        project_id: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<DomainPackRecord>, Self::Error> {
        self.find_domain_pack(tenant_id, project_id, name, version)
            .await
    }
}
