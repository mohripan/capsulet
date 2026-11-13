//! IR definitions, IR runs, and assurance certificates.

use super::super::*;

#[async_trait::async_trait]
impl IrStore for FakeStore {
    async fn insert_ir_definition_version(
        &self,
        tenant_id: &str,
        project_id: &str,
        definition: &capsulet_ir::definition::Definition,
        _admission: &capsulet_ir::admission::AdmissionRecord,
    ) -> Result<String, Self::Error> {
        let bytes =
            capsulet_ir::to_canonical_bytes(definition).map_err(|error| error.to_string())?;
        let digest = capsulet_ir::Digest::of(&bytes).to_string();
        let canonical = String::from_utf8(bytes).map_err(|error| error.to_string())?;
        let mut stored = self
            .ir_definitions
            .lock()
            .map_err(|error| error.to_string())?;
        if !stored.iter().any(|each| each.digest == digest) {
            stored.push(IrDefinitionVersion {
                tenant_id: tenant_id.to_string(),
                project_id: project_id.to_string(),
                definition_id: definition.id.as_str().to_string(),
                name: definition.name.clone(),
                version: definition.version.clone(),
                digest: digest.clone(),
                schema_version: definition.schema_version.to_string(),
                canonical_bytes: canonical,
            });
        }
        Ok(digest)
    }

    async fn get_ir_definition_version(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        digest: &str,
    ) -> Result<Option<IrDefinitionVersion>, Self::Error> {
        Ok(self
            .ir_definitions
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|each| each.digest == digest)
            .cloned())
    }

    async fn create_ir_run(
        &self,
        tenant_id: &str,
        project_id: &str,
        run_id: &str,
        definition_digest: &capsulet_ir::Digest,
        mode: capsulet_ir::definition::AssuranceMode,
        trace_parent: Option<&str>,
        inputs: &std::collections::BTreeMap<String, String>,
        at: capsulet_ir::RecordedTime,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        // Mirrors the real store: the run and its admission event are written
        // together, so a run whose log does not fold is not reachable.
        let record = IrRunRecord {
            tenant_id: tenant_id.to_string(),
            project_id: project_id.to_string(),
            id: run_id.to_string(),
            definition_digest: definition_digest.to_string(),
            status: "queued".to_string(),
            epoch: Epoch(0),
            lease_owner: None,
            next_position: if inputs.is_empty() { 1 } else { 2 },
            trace_parent: trace_parent.map(str::to_owned),
        };
        self.ir_runs
            .lock()
            .map_err(|error| error.to_string())?
            .push(record.clone());
        self.ir_run_events
            .lock()
            .map_err(|error| error.to_string())?
            .push((
                run_id.to_string(),
                RecordedEvent {
                    position: 0,
                    epoch: Epoch(0),
                    at,
                    event: RunEvent::Admitted {
                        definition: *definition_digest,
                        mode,
                    },
                },
            ));
        if !inputs.is_empty() {
            let bound = inputs
                .iter()
                .map(|(name, content)| (name.clone(), capsulet_ir::Digest::of(content.as_bytes())))
                .collect();
            self.ir_run_events
                .lock()
                .map_err(|error| error.to_string())?
                .push((
                    run_id.to_string(),
                    RecordedEvent {
                        position: 1,
                        epoch: Epoch(0),
                        at,
                        event: RunEvent::InputsBound { inputs: bound },
                    },
                ));
        }
        Ok(Some(record))
    }

    async fn get_ir_run(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        run_id: &str,
    ) -> Result<Option<IrRunRecord>, Self::Error> {
        Ok(self
            .ir_runs
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .find(|each| each.id == run_id)
            .cloned())
    }

    async fn list_ir_runs(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        _limit: i64,
    ) -> Result<Vec<IrRunRecord>, Self::Error> {
        Ok(self
            .ir_runs
            .lock()
            .map_err(|error| error.to_string())?
            .clone())
    }

    async fn load_ir_run_events(
        &self,
        _tenant_id: &str,
        _project_id: &str,
        run_id: &str,
    ) -> Result<Vec<RecordedEvent>, Self::Error> {
        Ok(self
            .ir_run_events
            .lock()
            .map_err(|error| error.to_string())?
            .iter()
            .filter(|(each, _)| each == run_id)
            .map(|(_, event)| event.clone())
            .collect())
    }
}
