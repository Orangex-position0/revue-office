use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::artifacts::{
    ArtifactDraft, ArtifactFinalization, ArtifactPublication, ArtifactPublicationRepository,
    ArtifactPublicationRepositoryError, ArtifactPublicationStatus, ArtifactService, FileStorage,
    FileStorageError, NewArtifactPublication, ReadyArtifactFile, StagedArtifactFile,
};
use revue_office_lib::infrastructure::filesystem::artifact_storage::LocalArtifactStorage;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;

#[derive(Clone, Copy)]
enum FailurePoint {
    Write,
    Validate,
    Promote,
}

struct FailingStorage {
    root: PathBuf,
    failure: FailurePoint,
    artifact_id: Arc<Mutex<Option<String>>>,
}

#[async_trait]
impl FileStorage for FailingStorage {
    fn staging_file(
        &self,
        artifact_id: &str,
        extension: &str,
    ) -> Result<StagedArtifactFile, FileStorageError> {
        *self.artifact_id.lock().unwrap() = Some(artifact_id.into());
        Ok(StagedArtifactFile {
            path: self
                .root
                .join("staging")
                .join(format!("{artifact_id}.{extension}"))
                .to_string_lossy()
                .into_owned(),
            final_path: self
                .root
                .join("ready")
                .join(format!("{artifact_id}.{extension}"))
                .to_string_lossy()
                .into_owned(),
        })
    }

    async fn write_staging(
        &self,
        file: &StagedArtifactFile,
        bytes: &[u8],
    ) -> Result<(), FileStorageError> {
        if matches!(self.failure, FailurePoint::Write) {
            return Err(io_error("write"));
        }
        tokio::fs::create_dir_all(PathBuf::from(&file.path).parent().unwrap())
            .await
            .unwrap();
        tokio::fs::write(&file.path, bytes).await.unwrap();
        Ok(())
    }

    async fn validate(&self, _file: &StagedArtifactFile) -> Result<(), FileStorageError> {
        if matches!(self.failure, FailurePoint::Validate) {
            Err(FileStorageError::InvalidFile("validation".into()))
        } else {
            Ok(())
        }
    }

    async fn promote(
        &self,
        _file: &StagedArtifactFile,
    ) -> Result<ReadyArtifactFile, FileStorageError> {
        Err(io_error("promote"))
    }

    async fn recover_staging(
        &self,
        _staging_path: &str,
    ) -> Result<Option<ReadyArtifactFile>, FileStorageError> {
        Ok(None)
    }

    async fn delete(&self, path: &str) -> Result<(), FileStorageError> {
        match tokio::fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(FileStorageError::Io(error)),
        }
    }
}

fn io_error(stage: &str) -> FileStorageError {
    FileStorageError::Io(std::io::Error::other(format!("injected {stage} failure")))
}

struct FinalizationFailureRepository {
    inner: Arc<SqliteSessionRepository>,
    last_id: Arc<Mutex<Option<String>>>,
    fail_reserve: bool,
}

#[async_trait]
impl ArtifactPublicationRepository for FinalizationFailureRepository {
    async fn reserve(
        &self,
        publication: NewArtifactPublication,
    ) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError> {
        *self.last_id.lock().unwrap() = Some(publication.id.clone());
        if self.fail_reserve {
            return Err(ArtifactPublicationRepositoryError::Unavailable(
                anyhow::anyhow!("injected reserve failure"),
            ));
        }
        self.inner.reserve(publication).await
    }

    async fn find(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        self.inner.find(id).await
    }

    async fn finalize(
        &self,
        _id: &str,
        _finalization: ArtifactFinalization,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        Err(ArtifactPublicationRepositoryError::Unavailable(
            anyhow::anyhow!("injected finalize failure"),
        ))
    }

    async fn fail(
        &self,
        id: &str,
        error: &str,
    ) -> Result<bool, ArtifactPublicationRepositoryError> {
        self.inner.fail(id, error).await
    }

    async fn pending(
        &self,
    ) -> Result<Vec<ArtifactPublication>, ArtifactPublicationRepositoryError> {
        self.inner.pending().await
    }
}

async fn repository(root: &std::path::Path) -> Arc<SqliteSessionRepository> {
    let url = format!("sqlite://{}?mode=rwc", root.join("artifacts.db").display());
    Arc::new(SqliteSessionRepository::connect(&url, 1).await.unwrap())
}

fn draft(title: &str) -> ArtifactDraft {
    ArtifactDraft {
        session_id: "session-1".into(),
        owner_id: "owner-1".into(),
        kind: "ppt".into(),
        title: title.into(),
        extension: "json".into(),
        content: serde_json::json!({"slides": []}),
        bytes: b"artifact bytes".to_vec(),
    }
}

#[tokio::test]
async fn artifact_failure_reconciliation_marks_each_storage_failure_without_ready_files() {
    for failure in [
        FailurePoint::Write,
        FailurePoint::Validate,
        FailurePoint::Promote,
    ] {
        let root =
            std::env::temp_dir().join(format!("revue-artifact-failure-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        let repository = repository(&root).await;
        let artifact_id = Arc::new(Mutex::new(None));
        let service = ArtifactService::new(
            repository.clone(),
            Arc::new(FailingStorage {
                root: root.join("files"),
                failure,
                artifact_id: artifact_id.clone(),
            }),
        );

        assert!(service.publish(draft("failure")).await.is_err());
        let id = artifact_id.lock().unwrap().clone().unwrap();
        let publication = repository.find(&id).await.unwrap().unwrap();
        assert_eq!(publication.status, ArtifactPublicationStatus::Failed);
        assert!(publication.final_path.is_none());
        assert!(!root.join("files/ready").exists());
        assert!(repository.pending().await.unwrap().is_empty());
        drop(service);
        drop(repository);
        let _ = std::fs::remove_dir_all(root);
    }
}

#[tokio::test]
async fn artifact_failure_reconciliation_reserve_failure_writes_no_files() {
    let root =
        std::env::temp_dir().join(format!("revue-artifact-reserve-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let repository = repository(&root).await;
    let service = ArtifactService::new(
        Arc::new(FinalizationFailureRepository {
            inner: repository.clone(),
            last_id: Arc::new(Mutex::new(None)),
            fail_reserve: true,
        }),
        Arc::new(LocalArtifactStorage::new(root.join("files"))),
    );

    assert!(service.publish(draft("reserve failure")).await.is_err());
    assert!(!root.join("files").exists());
    assert!(repository.pending().await.unwrap().is_empty());

    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn artifact_failure_reconciliation_compensates_finalize_failure() {
    let root =
        std::env::temp_dir().join(format!("revue-artifact-finalize-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let repository = repository(&root).await;
    let last_id = Arc::new(Mutex::new(None));
    let service = ArtifactService::new(
        Arc::new(FinalizationFailureRepository {
            inner: repository.clone(),
            last_id: last_id.clone(),
            fail_reserve: false,
        }),
        Arc::new(LocalArtifactStorage::new(root.join("files"))),
    );

    assert!(service.publish(draft("finalize failure")).await.is_err());
    let id = last_id.lock().unwrap().clone().unwrap();
    let publication = repository.find(&id).await.unwrap().unwrap();
    assert_eq!(publication.status, ArtifactPublicationStatus::Failed);
    assert!(
        !root
            .join("files/ready")
            .read_dir()
            .map(|mut files| files.next().is_some())
            .unwrap_or(false)
    );
    drop(service);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn artifact_failure_reconciliation_recovers_staged_and_promoted_files_on_restart() {
    let root =
        std::env::temp_dir().join(format!("revue-artifact-reconcile-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let repository = repository(&root).await;
    let storage = Arc::new(LocalArtifactStorage::new(root.join("files")));

    for (id, promote) in [("staged-artifact", false), ("promoted-artifact", true)] {
        let file = storage.staging_file(id, "json").unwrap();
        repository
            .reserve(NewArtifactPublication {
                id: id.into(),
                session_id: "session-1".into(),
                owner_id: "owner-1".into(),
                kind: "ppt".into(),
                title: id.into(),
                content: serde_json::json!({"slides": []}),
                staging_path: file.path.clone(),
            })
            .await
            .unwrap();
        storage.write_staging(&file, b"recoverable").await.unwrap();
        if promote {
            storage.promote(&file).await.unwrap();
        }
    }
    let missing = storage.staging_file("missing-artifact", "json").unwrap();
    repository
        .reserve(NewArtifactPublication {
            id: "missing-artifact".into(),
            session_id: "session-1".into(),
            owner_id: "owner-1".into(),
            kind: "ppt".into(),
            title: "missing".into(),
            content: serde_json::json!({}),
            staging_path: missing.path,
        })
        .await
        .unwrap();

    let restarted = ArtifactService::new(repository.clone(), storage);
    let report = restarted.reconcile_pending().await.unwrap();
    assert_eq!(report.recovered, 2);
    assert_eq!(report.failed, 1);
    assert_eq!(
        repository
            .find("staged-artifact")
            .await
            .unwrap()
            .unwrap()
            .status,
        ArtifactPublicationStatus::Ready
    );
    assert_eq!(
        repository
            .find("promoted-artifact")
            .await
            .unwrap()
            .unwrap()
            .status,
        ArtifactPublicationStatus::Ready
    );
    assert_eq!(
        repository
            .find("missing-artifact")
            .await
            .unwrap()
            .unwrap()
            .status,
        ArtifactPublicationStatus::Failed
    );
    assert!(repository.pending().await.unwrap().is_empty());
    assert_eq!(restarted.reconcile_pending().await.unwrap().recovered, 0);

    drop(restarted);
    drop(repository);
    let _ = std::fs::remove_dir_all(root);
}
