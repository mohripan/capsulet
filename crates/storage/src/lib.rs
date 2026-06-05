use std::{path::PathBuf, sync::Arc};

use async_trait::async_trait;
use capsulet_core::{ArtifactObjectKind, JobRunId};
use object_store::{
    ObjectStore as ObjectStoreBackend, aws::AmazonS3Builder, path::Path as ObjectPath,
};
use thiserror::Error;
use tokio::fs;

/// Minimal object storage boundary used for bundles, logs, and artifacts.
#[async_trait]
pub trait ObjectStore: Clone + Send + Sync + 'static {
    type Error: std::fmt::Display + Send + Sync + 'static;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error>;
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error>;
    async fn exists(&self, key: &str) -> Result<bool, Self::Error>;
}

/// Filesystem-backed object store for local development and tests.
#[derive(Debug, Clone)]
pub struct FilesystemObjectStore {
    root: Arc<PathBuf>,
}

impl FilesystemObjectStore {
    #[must_use]
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: Arc::new(root.into()),
        }
    }

    fn path_for_key(&self, key: &str) -> Result<PathBuf, ObjectStoreError> {
        validate_key(key)?;
        let mut path = self.root.as_ref().clone();
        for segment in key.split('/') {
            path.push(segment);
        }
        Ok(path)
    }
}

#[async_trait]
impl ObjectStore for FilesystemObjectStore {
    type Error = ObjectStoreError;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
        let path = self.path_for_key(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await?;
        }
        fs::write(path, bytes).await?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        let path = self.path_for_key(key)?;
        match fs::read(path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        let path = self.path_for_key(key)?;
        match fs::metadata(path).await {
            Ok(_) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error.into()),
        }
    }
}

/// S3-compatible object store for `MinIO` and external S3 endpoints.
#[derive(Clone)]
pub struct S3ObjectStore {
    inner: Arc<dyn ObjectStoreBackend>,
}

impl std::fmt::Debug for S3ObjectStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("S3ObjectStore")
            .finish_non_exhaustive()
    }
}

impl S3ObjectStore {
    /// Creates an S3-compatible store.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStoreError`] when the S3 client cannot be configured.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        bucket: &str,
        endpoint: Option<&str>,
        region: &str,
        access_key_id: &str,
        secret_access_key: &str,
        path_style: bool,
    ) -> Result<Self, ObjectStoreError> {
        let mut builder = AmazonS3Builder::new()
            .with_bucket_name(bucket)
            .with_region(region)
            .with_access_key_id(access_key_id)
            .with_secret_access_key(secret_access_key)
            .with_allow_http(true)
            .with_virtual_hosted_style_request(!path_style);
        if let Some(endpoint) = endpoint.filter(|value| !value.trim().is_empty()) {
            builder = builder.with_endpoint(endpoint);
        }
        Ok(Self {
            inner: Arc::new(builder.build().map_err(Box::new)?),
        })
    }
}

#[async_trait]
impl ObjectStore for S3ObjectStore {
    type Error = ObjectStoreError;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
        validate_key(key)?;
        self.inner
            .put(&ObjectPath::from(key), bytes.into())
            .await
            .map_err(Box::new)?;
        Ok(())
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        validate_key(key)?;
        match self.inner.get(&ObjectPath::from(key)).await {
            Ok(result) => Ok(Some(result.bytes().await.map_err(Box::new)?.to_vec())),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(error) => Err(ObjectStoreError::ObjectStore(Box::new(error))),
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        validate_key(key)?;
        match self.inner.head(&ObjectPath::from(key)).await {
            Ok(_) => Ok(true),
            Err(object_store::Error::NotFound { .. }) => Ok(false),
            Err(error) => Err(ObjectStoreError::ObjectStore(Box::new(error))),
        }
    }
}

/// Runtime object storage adapter selected from configuration.
#[derive(Debug, Clone)]
pub enum ConfiguredObjectStore {
    Filesystem(FilesystemObjectStore),
    S3(S3ObjectStore),
}

impl ConfiguredObjectStore {
    #[must_use]
    pub fn filesystem(root: impl Into<PathBuf>) -> Self {
        Self::Filesystem(FilesystemObjectStore::new(root))
    }

    /// Creates an S3-compatible configured object store.
    ///
    /// # Errors
    ///
    /// Returns [`ObjectStoreError`] when the S3 adapter cannot be configured.
    #[allow(clippy::too_many_arguments)]
    pub fn s3(
        bucket: &str,
        endpoint: Option<&str>,
        region: &str,
        access_key_id: &str,
        secret_access_key: &str,
        path_style: bool,
    ) -> Result<Self, ObjectStoreError> {
        Ok(Self::S3(S3ObjectStore::new(
            bucket,
            endpoint,
            region,
            access_key_id,
            secret_access_key,
            path_style,
        )?))
    }
}

#[async_trait]
impl ObjectStore for ConfiguredObjectStore {
    type Error = ObjectStoreError;

    async fn put(&self, key: &str, bytes: Vec<u8>) -> Result<(), Self::Error> {
        match self {
            Self::Filesystem(store) => store.put(key, bytes).await,
            Self::S3(store) => store.put(key, bytes).await,
        }
    }

    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>, Self::Error> {
        match self {
            Self::Filesystem(store) => store.get(key).await,
            Self::S3(store) => store.get(key).await,
        }
    }

    async fn exists(&self, key: &str) -> Result<bool, Self::Error> {
        match self {
            Self::Filesystem(store) => store.exists(key).await,
            Self::S3(store) => store.exists(key).await,
        }
    }
}

/// Builds a deterministic object key for one run-scoped object.
///
/// # Errors
///
/// Returns an error when `name` contains path traversal or empty segments.
pub fn run_object_key(
    run_id: &JobRunId,
    kind: ArtifactObjectKind,
    name: &str,
) -> Result<String, ObjectStoreError> {
    validate_key(name)?;
    Ok(format!("{}/{}/{}", kind.prefix(), run_id.as_str(), name))
}

fn validate_key(key: &str) -> Result<(), ObjectStoreError> {
    if key.trim().is_empty() {
        return Err(ObjectStoreError::InvalidKey(
            "object key cannot be empty".to_string(),
        ));
    }
    if key.starts_with('/') || key.ends_with('/') {
        return Err(ObjectStoreError::InvalidKey(format!(
            "object key must be relative: {key}"
        )));
    }
    if key
        .split('/')
        .any(|segment| segment.is_empty() || segment == "." || segment == "..")
    {
        return Err(ObjectStoreError::InvalidKey(format!(
            "object key contains an invalid path segment: {key}"
        )));
    }
    Ok(())
}

#[derive(Debug, Error)]
pub enum ObjectStoreError {
    #[error("invalid object key: {0}")]
    InvalidKey(String),
    #[error("filesystem object store error: {0}")]
    Io(#[from] std::io::Error),
    #[error("object store error: {0}")]
    ObjectStore(#[from] Box<object_store::Error>),
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use capsulet_core::{ArtifactObjectKind, JobRunId};

    use super::{FilesystemObjectStore, ObjectStore, run_object_key};

    fn temp_root() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("capsulet-storage-test-{nanos}"))
    }

    #[tokio::test]
    async fn puts_gets_and_checks_objects() {
        let store = FilesystemObjectStore::new(temp_root());

        store
            .put("artifacts/run_1/report.txt", b"hello".to_vec())
            .await
            .expect("put object");

        assert!(
            store
                .exists("artifacts/run_1/report.txt")
                .await
                .expect("exists")
        );
        assert_eq!(
            store
                .get("artifacts/run_1/report.txt")
                .await
                .expect("get object"),
            Some(b"hello".to_vec())
        );
    }

    #[test]
    fn builds_run_scoped_keys() {
        let key = run_object_key(
            &JobRunId::new("run_1").expect("run id"),
            ArtifactObjectKind::Artifact,
            "report.txt",
        )
        .expect("object key");

        assert_eq!(key, "artifacts/run_1/report.txt");
    }

    #[test]
    fn rejects_path_traversal() {
        let error = run_object_key(
            &JobRunId::new("run_1").expect("run id"),
            ArtifactObjectKind::Artifact,
            "../secret.txt",
        );

        assert!(error.is_err());
    }
}
