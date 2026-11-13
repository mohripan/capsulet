//! The domain packs a project holds.

use super::super::*;

#[async_trait::async_trait]
impl PackStore for FakeStore {
    async fn insert_domain_pack(&self, record: &DomainPackRecord) -> Result<bool, Self::Error> {
        let mut packs = self.domain_packs.lock().expect("packs mutex");
        let held = packs.iter().any(|stored| {
            stored.tenant_id == record.tenant_id
                && stored.project_id == record.project_id
                && stored.name == record.name
                && stored.version == record.version
        });
        if held {
            return Ok(false);
        }
        packs.push(record.clone());
        Ok(true)
    }

    async fn list_domain_packs(
        &self,
        tenant_id: &str,
        project_id: &str,
    ) -> Result<Vec<DomainPackRecord>, Self::Error> {
        Ok(self
            .domain_packs
            .lock()
            .expect("packs mutex")
            .iter()
            .filter(|record| record.tenant_id == tenant_id && record.project_id == project_id)
            .cloned()
            .collect())
    }

    async fn find_domain_pack(
        &self,
        tenant_id: &str,
        project_id: &str,
        name: &str,
        version: &str,
    ) -> Result<Option<DomainPackRecord>, Self::Error> {
        Ok(self
            .domain_packs
            .lock()
            .expect("packs mutex")
            .iter()
            .find(|record| {
                record.tenant_id == tenant_id
                    && record.project_id == project_id
                    && record.name == name
                    && record.version == version
            })
            .cloned())
    }
}
