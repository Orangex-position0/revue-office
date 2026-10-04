use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::assets::*;
use revue_office_lib::application::identity::{Actor, ActorId};
use revue_office_lib::infrastructure::filesystem::assets::LocalAssetStorage;
use revue_office_lib::infrastructure::persistence::sqlite::assets::SqliteAssetRepository;

#[derive(Default)]
struct FakeRepository {
    assets: Mutex<HashMap<String, Asset>>,
    folders: Mutex<HashMap<String, Folder>>,
    fail_create: Mutex<bool>,
    fail_delete: Mutex<bool>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl AssetRepository for FakeRepository {
    async fn create(&self, value: NewAssetRecord) -> Result<Asset, AssetError> {
        self.events.lock().unwrap().push("repository.create");
        if *self.fail_create.lock().unwrap() {
            return Err(AssetError::Repository(anyhow::anyhow!("create")));
        }
        let asset = Asset {
            id: "asset-1".into(),
            owner_id: value.owner_id.0,
            name: value.name,
            file_path: value.location.0,
            file_type: value.file_type,
            file_size: value.file_size,
            folder_id: value.folder_id.map(|v| v.0),
            description: value.description,
            metadata: value.metadata,
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        self.assets
            .lock()
            .unwrap()
            .insert(asset.id.clone(), asset.clone());
        Ok(asset)
    }
    async fn get(&self, owner: &ActorId, id: &AssetId) -> Result<Option<Asset>, AssetError> {
        Ok(self
            .assets
            .lock()
            .unwrap()
            .get(&id.0)
            .filter(|v| v.owner_id == owner.0)
            .cloned())
    }
    async fn list(&self, q: AssetListQuery) -> Result<Vec<Asset>, AssetError> {
        Ok(self
            .assets
            .lock()
            .unwrap()
            .values()
            .filter(|v| {
                v.owner_id == q.owner_id.0
                    && v.folder_id == q.folder_id.as_ref().map(|f| f.0.clone())
            })
            .cloned()
            .collect())
    }
    async fn search(&self, q: AssetSearchQuery) -> Result<Vec<Asset>, AssetError> {
        let query = q.query.unwrap_or_default();
        Ok(self
            .assets
            .lock()
            .unwrap()
            .values()
            .filter(|v| v.owner_id == q.owner_id.0 && v.name.contains(&query))
            .cloned()
            .collect())
    }
    async fn stats(&self, owner: &ActorId) -> Result<AssetStats, AssetError> {
        let assets = self.assets.lock().unwrap();
        let values = assets
            .values()
            .filter(|v| v.owner_id == owner.0)
            .collect::<Vec<_>>();
        let mut by_type = HashMap::new();
        for value in &values {
            *by_type.entry(value.file_type.clone()).or_insert(0) += 1;
        }
        Ok(AssetStats {
            by_type,
            total_size: values.iter().map(|v| v.file_size).sum(),
            total_files: values.len() as i64,
        })
    }
    async fn delete(&self, owner: &ActorId, id: &AssetId) -> Result<bool, AssetError> {
        self.events.lock().unwrap().push("repository.delete");
        if *self.fail_delete.lock().unwrap() {
            return Err(AssetError::Repository(anyhow::anyhow!("delete")));
        }
        let owned = self
            .assets
            .lock()
            .unwrap()
            .get(&id.0)
            .is_some_and(|v| v.owner_id == owner.0);
        if owned {
            self.assets.lock().unwrap().remove(&id.0);
        }
        Ok(owned)
    }
    async fn get_folder(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<Folder>, AssetError> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .get(&id.0)
            .filter(|v| v.owner_id == owner.0)
            .cloned())
    }
    async fn list_folders(
        &self,
        owner: &ActorId,
        parent: Option<&FolderId>,
    ) -> Result<Vec<Folder>, AssetError> {
        Ok(self
            .folders
            .lock()
            .unwrap()
            .values()
            .filter(|v| {
                v.owner_id == owner.0 && v.parent_id.as_deref() == parent.map(|v| v.0.as_str())
            })
            .cloned()
            .collect())
    }
    async fn create_folder(&self, value: NewFolder) -> Result<Folder, AssetError> {
        let folder = Folder {
            id: "folder-1".into(),
            owner_id: value.owner_id.0,
            name: value.name,
            parent_id: value.parent_id.map(|v| v.0),
            created_at: "now".into(),
            updated_at: "now".into(),
        };
        self.folders
            .lock()
            .unwrap()
            .insert(folder.id.clone(), folder.clone());
        Ok(folder)
    }
    async fn folder_tree_manifest(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<FolderTreeManifest>, AssetError> {
        if self.get_folder(owner, id).await?.is_none() {
            return Ok(None);
        }
        let assets = self
            .assets
            .lock()
            .unwrap()
            .values()
            .filter(|v| v.owner_id == owner.0 && v.folder_id.as_deref() == Some(&id.0))
            .cloned()
            .collect();
        Ok(Some(FolderTreeManifest {
            folder_ids: vec![id.clone()],
            assets,
        }))
    }
    async fn delete_folder_tree(
        &self,
        owner: &ActorId,
        manifest: &FolderTreeManifest,
    ) -> Result<bool, AssetError> {
        self.events.lock().unwrap().push("repository.delete_folder");
        if *self.fail_delete.lock().unwrap() {
            return Err(AssetError::Repository(anyhow::anyhow!("delete folder")));
        }
        for asset in &manifest.assets {
            self.assets.lock().unwrap().remove(&asset.id);
        }
        let mut deleted = false;
        for folder in &manifest.folder_ids {
            if self
                .folders
                .lock()
                .unwrap()
                .get(&folder.0)
                .is_some_and(|v| v.owner_id == owner.0)
            {
                self.folders.lock().unwrap().remove(&folder.0);
                deleted = true;
            }
        }
        Ok(deleted)
    }
}

struct FakeStorage {
    bytes: Mutex<HashMap<String, Vec<u8>>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}
impl FakeStorage {
    fn new(events: Arc<Mutex<Vec<&'static str>>>) -> Self {
        Self {
            bytes: Mutex::new(HashMap::new()),
            events,
        }
    }
}
#[async_trait]
impl AssetStorage for FakeStorage {
    async fn write(&self, request: AssetWrite) -> Result<StoredAsset, AssetError> {
        self.events.lock().unwrap().push("storage.write");
        let path = format!("/{}/asset", request.owner_id.0);
        self.bytes
            .lock()
            .unwrap()
            .insert(path.clone(), request.bytes);
        Ok(StoredAsset {
            location: AssetLocation(path),
        })
    }
    async fn read(&self, location: &AssetLocation) -> Result<Vec<u8>, AssetError> {
        self.events.lock().unwrap().push("storage.read");
        self.bytes
            .lock()
            .unwrap()
            .get(&location.0)
            .cloned()
            .ok_or(AssetError::NotFound)
    }
    async fn quarantine(
        &self,
        location: &AssetLocation,
    ) -> Result<Option<QuarantinedAsset>, AssetError> {
        self.events.lock().unwrap().push("storage.quarantine");
        let mut bytes = self.bytes.lock().unwrap();
        let Some(value) = bytes.remove(&location.0) else {
            return Ok(None);
        };
        let quarantine = format!("{}.trash", location.0);
        bytes.insert(quarantine.clone(), value);
        Ok(Some(QuarantinedAsset {
            original: location.clone(),
            quarantine: AssetLocation(quarantine),
        }))
    }
    async fn restore(&self, asset: &QuarantinedAsset) -> Result<(), AssetError> {
        self.events.lock().unwrap().push("storage.restore");
        let value = self
            .bytes
            .lock()
            .unwrap()
            .remove(&asset.quarantine.0)
            .ok_or(AssetError::NotFound)?;
        self.bytes
            .lock()
            .unwrap()
            .insert(asset.original.0.clone(), value);
        Ok(())
    }
    async fn purge(&self, asset: QuarantinedAsset) -> Result<(), AssetError> {
        self.events.lock().unwrap().push("storage.purge");
        self.bytes.lock().unwrap().remove(&asset.quarantine.0);
        Ok(())
    }
}

struct FakeExtractor {
    events: Arc<Mutex<Vec<&'static str>>>,
}
#[async_trait]
impl AssetContentExtractor for FakeExtractor {
    async fn extract_text(&self, input: AssetContentInput) -> Result<ExtractedText, AssetError> {
        self.events.lock().unwrap().push("extract");
        Ok(ExtractedText {
            text: String::from_utf8_lossy(&input.bytes).into(),
            parser: "fake".into(),
            truncated: false,
        })
    }
    async fn extract_structured(
        &self,
        _input: AssetContentInput,
    ) -> Result<StructuredPreview, AssetError> {
        Ok(StructuredPreview {
            preview_type: "text".into(),
            data: serde_json::json!({"text":"ok"}),
            parser: "fake".into(),
            truncated: false,
        })
    }
}

fn service(
    fail_create: bool,
    fail_delete: bool,
) -> (
    Arc<AssetApplicationService>,
    Arc<FakeRepository>,
    Arc<FakeStorage>,
    Arc<Mutex<Vec<&'static str>>>,
) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let repository = Arc::new(FakeRepository {
        fail_create: Mutex::new(fail_create),
        fail_delete: Mutex::new(fail_delete),
        events: events.clone(),
        ..Default::default()
    });
    let storage = Arc::new(FakeStorage::new(events.clone()));
    let service = Arc::new(AssetApplicationService::new(
        repository.clone(),
        storage.clone(),
        Arc::new(FakeExtractor {
            events: events.clone(),
        }),
    ));
    (service, repository, storage, events)
}

#[test]
fn asset_handler_keeps_public_route_contract() {
    let source = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/transport/http/handlers/assets.rs"),
    )
    .unwrap();
    for route in [
        "/api/files",
        "/api/files/search",
        "/api/files/stats",
        "/api/files/extract",
        "/api/files/upload",
        "/api/files/:id/content",
        "/api/files/:id/thumbnail",
        "/api/files/:id/preview",
        "/api/files/:id/stream",
        "/api/files/:id",
        "/api/files/:id/download",
        "/api/files/folders/list",
        "/api/folders",
        "/api/folders/:id",
    ] {
        assert!(source.contains(route), "missing compatible route {route}");
    }
}

#[tokio::test]
async fn upload_extracts_then_writes_then_creates_metadata() {
    let (service, _, _, events) = service(false, false);
    let actor = Actor::user("owner");
    let asset = service
        .upload(
            &actor,
            NewAsset {
                name: "a.txt".into(),
                mime_type: "text/plain".into(),
                bytes: b"hello".to_vec(),
                folder_id: None,
                description: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(asset.name, "a.txt");
    assert_eq!(
        *events.lock().unwrap(),
        vec!["extract", "storage.write", "repository.create"]
    );
}

#[tokio::test]
async fn repository_failure_compensates_written_file() {
    let (service, _, storage, events) = service(true, false);
    let actor = Actor::user("owner");
    assert!(
        service
            .upload(
                &actor,
                NewAsset {
                    name: "a.txt".into(),
                    mime_type: "text/plain".into(),
                    bytes: b"hello".to_vec(),
                    folder_id: None,
                    description: None
                }
            )
            .await
            .is_err()
    );
    assert!(storage.bytes.lock().unwrap().is_empty());
    assert_eq!(
        *events.lock().unwrap(),
        vec![
            "extract",
            "storage.write",
            "repository.create",
            "storage.quarantine",
            "storage.purge"
        ]
    );
}

#[tokio::test]
async fn delete_quarantines_before_metadata_and_restores_on_failure() {
    let (service, repository, storage, events) = service(false, true);
    let actor = Actor::user("owner");
    repository.assets.lock().unwrap().insert(
        "asset".into(),
        Asset {
            id: "asset".into(),
            owner_id: "owner".into(),
            name: "a.txt".into(),
            file_path: "/owner/asset".into(),
            file_type: "doc".into(),
            file_size: 5,
            folder_id: None,
            description: None,
            metadata: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        },
    );
    storage
        .bytes
        .lock()
        .unwrap()
        .insert("/owner/asset".into(), b"hello".to_vec());
    assert!(
        service
            .delete(&actor, AssetId("asset".into()))
            .await
            .is_err()
    );
    assert!(storage.bytes.lock().unwrap().contains_key("/owner/asset"));
    assert_eq!(
        *events.lock().unwrap(),
        vec!["storage.quarantine", "repository.delete", "storage.restore"]
    );
}

#[tokio::test]
async fn owner_scope_prevents_cross_tenant_read() {
    let (service, repository, _, _) = service(false, false);
    repository.assets.lock().unwrap().insert(
        "asset".into(),
        Asset {
            id: "asset".into(),
            owner_id: "owner".into(),
            name: "a".into(),
            file_path: "/a".into(),
            file_type: "doc".into(),
            file_size: 1,
            folder_id: None,
            description: None,
            metadata: None,
            created_at: "now".into(),
            updated_at: "now".into(),
        },
    );
    assert!(matches!(
        service
            .get(&Actor::user("other"), AssetId("asset".into()))
            .await,
        Err(AssetError::NotFound)
    ));
}

#[tokio::test]
async fn sqlite_repository_reads_historical_rows_after_reconnect() {
    let path = std::env::temp_dir().join(format!("revue-assets-{}.db", uuid::Uuid::new_v4()));
    let url = format!("sqlite://{}?mode=rwc", path.to_string_lossy());
    let repository = SqliteAssetRepository::connect(&url, 1).await.unwrap();
    drop(repository);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind("owner").bind("asset-owner").bind("test-only").bind("now").bind("now").execute(&pool).await.unwrap();
    pool.close().await;
    let repository = SqliteAssetRepository::connect(&url, 1).await.unwrap();
    let owner = ActorId("owner".into());
    let created = repository
        .create(NewAssetRecord {
            owner_id: owner.clone(),
            name: "legacy.txt".into(),
            location: AssetLocation("C:/legacy/absolute.txt".into()),
            file_type: "doc".into(),
            file_size: 7,
            folder_id: None,
            description: Some("old".into()),
            metadata: Some(serde_json::json!({"unknown":"preserved"})),
        })
        .await
        .unwrap();
    drop(repository);
    let repository = SqliteAssetRepository::connect(&url, 1).await.unwrap();
    let loaded = repository
        .get(&owner, &AssetId(created.id))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.file_path, "C:/legacy/absolute.txt");
    assert_eq!(loaded.metadata.unwrap()["unknown"], "preserved");
    repository_contract(&repository, &owner).await;
    drop(repository);
    let _ = std::fs::remove_file(path);
}

async fn repository_contract(repository: &dyn AssetRepository, owner: &ActorId) {
    let folder = repository
        .create_folder(NewFolder {
            owner_id: owner.clone(),
            name: "Contract".into(),
            parent_id: None,
        })
        .await
        .unwrap();
    let asset = repository
        .create(NewAssetRecord {
            owner_id: owner.clone(),
            name: "contract.txt".into(),
            location: AssetLocation("/history/contract.txt".into()),
            file_type: "doc".into(),
            file_size: 9,
            folder_id: Some(FolderId(folder.id.clone())),
            description: Some("searchable".into()),
            metadata: Some(serde_json::json!({"unknown":"kept"})),
        })
        .await
        .unwrap();
    assert_eq!(
        repository
            .get(owner, &AssetId(asset.id.clone()))
            .await
            .unwrap()
            .unwrap()
            .metadata
            .unwrap()["unknown"],
        "kept"
    );
    assert_eq!(
        repository
            .list(AssetListQuery {
                owner_id: owner.clone(),
                folder_id: Some(FolderId(folder.id.clone())),
            })
            .await
            .unwrap()
            .len(),
        1
    );
    assert_eq!(
        repository
            .search(AssetSearchQuery {
                owner_id: owner.clone(),
                query: Some("searchable".into()),
            })
            .await
            .unwrap()
            .len(),
        1
    );
    let stats = repository.stats(owner).await.unwrap();
    assert!(stats.total_files >= 1);
    assert!(stats.total_size >= 9);
    assert!(stats.by_type.get("doc").copied().unwrap_or_default() >= 1);
    let manifest = repository
        .folder_tree_manifest(owner, &FolderId(folder.id.clone()))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(manifest.assets.len(), 1);
    assert!(
        repository
            .delete_folder_tree(owner, &manifest)
            .await
            .unwrap()
    );
    assert!(
        repository
            .get(owner, &AssetId(asset.id))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn local_storage_reads_historical_absolute_path_and_compensates() {
    let root = std::env::temp_dir().join(format!("revue-storage-{}", uuid::Uuid::new_v4()));
    let historical = root.join("historical.txt");
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(&historical, b"history").await.unwrap();
    let storage = LocalAssetStorage::new(root.clone());
    let location = AssetLocation(historical.to_string_lossy().into_owned());
    assert_eq!(storage.read(&location).await.unwrap(), b"history");
    let quarantined = storage.quarantine(&location).await.unwrap().unwrap();
    assert!(storage.read(&location).await.is_err());
    storage.restore(&quarantined).await.unwrap();
    assert_eq!(storage.read(&location).await.unwrap(), b"history");
    let quarantined = storage.quarantine(&location).await.unwrap().unwrap();
    storage.purge(quarantined).await.unwrap();
    assert!(storage.read(&location).await.is_err());
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
#[ignore = "requires REVUE_ALLOW_MYSQL_TEST=1 and isolated MYSQL_TEST_DATABASE_URL"]
async fn mysql_asset_contract_requires_isolated_database() {
    assert_eq!(
        std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref(),
        Ok("1"),
        "REVUE_ALLOW_MYSQL_TEST=1 is required"
    );
    let url = std::env::var("MYSQL_TEST_DATABASE_URL").expect("MYSQL_TEST_DATABASE_URL");
    let database = url
        .rsplit('/')
        .next()
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    assert!(
        database.starts_with("test_") || database.ends_with("_test"),
        "refusing non-test database"
    );
    let repository = revue_office_lib::infrastructure::persistence::mysql::assets::MySqlAssetRepository::connect(&url, 1).await.unwrap();
    let owner = ActorId(uuid::Uuid::new_v4().to_string());
    let pool = sqlx::mysql::MySqlPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query("INSERT INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(&owner.0).bind(format!("user-{}", uuid::Uuid::new_v4())).bind("test-only").bind("now").bind("now").execute(&pool).await.unwrap();
    pool.close().await;
    repository_contract(&repository, &owner).await;
}
