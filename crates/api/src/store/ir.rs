//! IR definitions, IR runs and their events, and assurance certificates with the evidence they
//! cite.

use async_trait::async_trait;
use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::correctness::certificate::Certificate as AssuranceCertificate;
use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::digest::Digest;
use capsulet_postgres::{
    EvidenceLocation, IrDefinitionVersion, IrRunRecord, PostgresStore, StoredCertificate,
};
use capsulet_runtime::RecordedEvent;

use super::StoreBase;

/// IR definitions, IR runs and their events, and assurance certificates with the evidence they
/// cite.
#[async_trait]
pub trait IrStore: StoreBase {
    /// Registers an admitted IR definition version and returns its digest.
    ///
    /// Default implementations return nothing, because a store that does not
    /// implement the correctness plane should say so by having nothing to show
    /// rather than by pretending a definition was stored.
    async fn insert_ir_definition_version(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _definition: &Definition,
        _admission: &AdmissionRecord,
    ) -> Result<String, Self::Error> {
        Ok(String::new())
    }

    async fn list_ir_definition_versions(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<IrDefinitionVersion>, Self::Error> {
        Ok(Vec::new())
    }

    async fn get_ir_definition_version(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _digest: &str,
    ) -> Result<Option<IrDefinitionVersion>, Self::Error> {
        Ok(None)
    }

    /// Enqueues a run of a stored definition version.
    ///
    /// Enqueues rather than starts: the run is written with its admission
    /// event and no lease, and the graph worker picks it up. Nothing in the API
    /// advances a run, and a contract test refuses the code that would.
    #[allow(
        clippy::too_many_arguments,
        reason = "each is a column of the run or its admission event"
    )]
    async fn create_ir_run(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _run_id: &str,
        _definition_digest: &Digest,
        _mode: AssuranceMode,
        _trace_parent: Option<&str>,
        _at: RecordedTime,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        Ok(None)
    }

    async fn get_ir_run(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _run_id: &str,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        Ok(None)
    }

    async fn list_ir_runs(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<IrRunRecord>, Self::Error> {
        Ok(Vec::new())
    }

    async fn load_ir_run_events(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _run_id: &str,
    ) -> Result<Vec<RecordedEvent>, Self::Error> {
        Ok(Vec::new())
    }

    async fn insert_assurance_certificate(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _certificate: &AssuranceCertificate,
    ) -> Result<(), Self::Error> {
        Ok(())
    }

    async fn list_assurance_certificates(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<StoredCertificate>, Self::Error> {
        Ok(Vec::new())
    }

    async fn get_assurance_certificate(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _id: &str,
    ) -> Result<Option<StoredCertificate>, Self::Error> {
        Ok(None)
    }

    async fn get_assurance_evidence(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _digest: &str,
    ) -> Result<Option<EvidenceLocation>, Self::Error> {
        Ok(None)
    }
}

#[async_trait]
impl IrStore for PostgresStore {
    async fn insert_ir_definition_version(
        &self,
        tenant_id: &str,
        project_id: &str,
        definition: &Definition,
        admission: &AdmissionRecord,
    ) -> Result<String, Self::Error> {
        self.insert_ir_definition_version(tenant_id, project_id, definition, admission)
            .await
            .map(|digest| digest.to_string())
    }

    async fn list_ir_definition_versions(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IrDefinitionVersion>, Self::Error> {
        self.list_ir_definition_versions(tenant_id, project_id, limit)
            .await
    }

    async fn get_ir_definition_version(
        &self,
        tenant_id: &str,
        project_id: &str,
        digest: &str,
    ) -> Result<Option<IrDefinitionVersion>, Self::Error> {
        self.get_ir_definition_version(tenant_id, project_id, digest)
            .await
    }

    async fn create_ir_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        definition_digest: &Digest,
        mode: AssuranceMode,
        trace_parent: Option<&str>,
        at: RecordedTime,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        self.create_ir_run(
            tenant_id,
            project_id,
            run_id,
            definition_digest,
            mode,
            trace_parent,
            at,
        )
        .await
        .map(Some)
    }

    async fn get_ir_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        self.get_ir_run(tenant_id, project_id, run_id).await
    }

    async fn list_ir_runs(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<IrRunRecord>, Self::Error> {
        self.list_ir_runs(tenant_id, project_id, limit).await
    }

    async fn load_ir_run_events(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
    ) -> Result<Vec<RecordedEvent>, Self::Error> {
        self.load_ir_run_events(tenant_id, project_id, run_id).await
    }

    async fn insert_assurance_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        certificate: &AssuranceCertificate,
    ) -> Result<(), Self::Error> {
        self.insert_assurance_certificate(tenant_id, project_id, certificate)
            .await
    }

    async fn list_assurance_certificates(
        &self,
        tenant_id: &str,
        project_id: &str,
        limit: i64,
    ) -> Result<Vec<StoredCertificate>, Self::Error> {
        self.list_assurance_certificates(tenant_id, project_id, limit)
            .await
    }

    async fn get_assurance_certificate(
        &self,
        tenant_id: &str,
        project_id: &str,
        id: &str,
    ) -> Result<Option<StoredCertificate>, Self::Error> {
        self.get_assurance_certificate(tenant_id, project_id, id)
            .await
    }

    async fn get_assurance_evidence(
        &self,
        tenant_id: &str,
        project_id: &str,
        digest: &str,
    ) -> Result<Option<EvidenceLocation>, Self::Error> {
        self.get_assurance_evidence(tenant_id, project_id, digest)
            .await
    }
}
