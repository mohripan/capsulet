//! Reasoning certificates and the decisions recorded on their readings.

use super::super::*;

#[async_trait::async_trait]
impl ReasoningStore for FakeStore {
    async fn insert_certificate(&self, record: &CertificateRecord) -> Result<(), Self::Error> {
        let mut certificates = self
            .reasoning_certificates
            .lock()
            .expect("certificates mutex");
        if !certificates.iter().any(|stored| stored.id == record.id) {
            certificates.push(record.clone());
        }
        Ok(())
    }

    async fn list_certificates(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<CertificateRecord>, Self::Error> {
        Ok(self
            .reasoning_certificates
            .lock()
            .expect("certificates mutex")
            .iter()
            .rev()
            .filter(|record| record.tenant_id == tenant_id && record.project_id == project_id)
            .take(usize::try_from(limit).unwrap_or(0))
            .cloned()
            .collect())
    }

    async fn find_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<CertificateRecord>, Self::Error> {
        Ok(self
            .reasoning_certificates
            .lock()
            .expect("certificates mutex")
            .iter()
            .find(|record| {
                record.id == id && record.tenant_id == tenant_id && record.project_id == project_id
            })
            .cloned())
    }

    async fn insert_reading_review(
        &self,
        record: &ReadingReviewRecord,
    ) -> Result<bool, Self::Error> {
        let mut reviews = self.reading_reviews.lock().expect("reviews mutex");
        let decided = reviews.iter().any(|stored| {
            stored.tenant_id == record.tenant_id
                && stored.project_id == record.project_id
                && stored.certificate_id == record.certificate_id
                && stored.review.reading == record.review.reading
        });
        if decided {
            return Ok(false);
        }
        reviews.push(record.clone());
        Ok(true)
    }

    async fn list_reading_reviews(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate_ids: &[String],
    ) -> Result<Vec<ReadingReviewRecord>, Self::Error> {
        Ok(self
            .reading_reviews
            .lock()
            .expect("reviews mutex")
            .iter()
            .filter(|record| {
                record.tenant_id == tenant_id
                    && record.project_id == project_id
                    && certificate_ids.contains(&record.certificate_id)
            })
            .cloned()
            .collect())
    }
}
