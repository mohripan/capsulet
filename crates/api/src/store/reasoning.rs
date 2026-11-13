//! Reasoning certificates from the ask loop, and the decisions people record on their readings.

use async_trait::async_trait;
use capsulet_postgres::{CertificateRecord, PostgresStore, ReadingReviewRecord};

use super::StoreBase;

/// Reasoning certificates from the ask loop, and the decisions people record on their readings.
#[async_trait]
pub trait ReasoningStore: StoreBase {
    async fn insert_certificate(&self, _record: &CertificateRecord) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn list_certificates(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<CertificateRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn find_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<CertificateRecord>, Self::Error>;

    /// Records a decision on a reading; `false` when that reading already has one.
    async fn insert_reading_review(
        &self,
        record: &ReadingReviewRecord,
    ) -> Result<bool, Self::Error>;

    async fn list_reading_reviews(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate_ids: &[String],
    ) -> Result<Vec<ReadingReviewRecord>, Self::Error>;
}

#[async_trait]
impl ReasoningStore for PostgresStore {
    async fn insert_certificate(&self, record: &CertificateRecord) -> Result<(), Self::Error> {
        self.insert_certificate(record).await
    }

    async fn list_certificates(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CertificateRecord>, Self::Error> {
        self.list_certificates(tenant_id, project_id, limit).await
    }

    async fn find_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<CertificateRecord>, Self::Error> {
        self.find_certificate(tenant_id, project_id, id).await
    }

    async fn insert_reading_review(
        &self,
        record: &ReadingReviewRecord,
    ) -> Result<bool, Self::Error> {
        self.insert_reading_review(record).await
    }

    async fn list_reading_reviews(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate_ids: &[String],
    ) -> Result<Vec<ReadingReviewRecord>, Self::Error> {
        self.list_reading_reviews(tenant_id, project_id, certificate_ids)
            .await
    }
}
