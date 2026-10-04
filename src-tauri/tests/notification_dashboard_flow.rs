use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::assets::{
    AssetApplicationService, AssetContentExtractor, AssetContentInput, AssetError, AssetLocation,
    AssetStorage, AssetWrite, ExtractedText, QuarantinedAsset, StoredAsset, StructuredPreview,
};
use revue_office_lib::application::conversations::SessionApplicationService;
use revue_office_lib::application::conversations::model::NewConversation;
use revue_office_lib::application::dashboard::{DashboardApplicationService, DashboardError};
use revue_office_lib::application::identity::Actor;
use revue_office_lib::application::notifications::{
    NotificationApplicationService, NotificationError, NotificationRepository,
};
use revue_office_lib::application::projects::ProjectApplicationService;
use revue_office_lib::capabilities::presentation::{
    PresentationCapability, PresentationExport, PresentationExportError, PresentationExporter,
    PresentationPlan, PresentationPlanRequest, PresentationPlanner, PresentationPlannerError,
    PresentationProject, PresentationStore, PresentationStoreError,
};
use revue_office_lib::infrastructure::persistence::mysql::notifications::MySqlNotificationRepository;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;
use revue_office_lib::infrastructure::persistence::sqlite::assets::SqliteAssetRepository;
use revue_office_lib::infrastructure::persistence::sqlite::notifications::SqliteNotificationRepository;
use revue_office_lib::infrastructure::persistence::sqlite::projects::SqliteProjectRepository;
use sqlx::{MySqlPool, SqlitePool};

struct UnusedStorage;

#[async_trait]
impl AssetStorage for UnusedStorage {
    async fn write(&self, _: AssetWrite) -> Result<StoredAsset, AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
    async fn read(&self, _: &AssetLocation) -> Result<Vec<u8>, AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
    async fn quarantine(&self, _: &AssetLocation) -> Result<Option<QuarantinedAsset>, AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
    async fn restore(&self, _: &QuarantinedAsset) -> Result<(), AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
    async fn purge(&self, _: QuarantinedAsset) -> Result<(), AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
}

struct UnusedExtractor;

#[async_trait]
impl AssetContentExtractor for UnusedExtractor {
    async fn extract_text(&self, _: AssetContentInput) -> Result<ExtractedText, AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
    async fn extract_structured(
        &self,
        _: AssetContentInput,
    ) -> Result<StructuredPreview, AssetError> {
        Err(AssetError::Unsupported("unused in dashboard".into()))
    }
}

struct UnusedPlanner;

#[async_trait]
impl PresentationPlanner for UnusedPlanner {
    async fn plan(
        &self,
        _: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationPlannerError> {
        Err(PresentationPlannerError::InvalidResponse(
            "unused in dashboard".into(),
        ))
    }
}

#[derive(Default)]
struct MemoryPresentationStore {
    projects: Mutex<Vec<PresentationProject>>,
}

#[async_trait]
impl PresentationStore for MemoryPresentationStore {
    async fn save(&self, project: &PresentationProject) -> Result<(), PresentationStoreError> {
        let mut projects = self.projects.lock().unwrap();
        projects.retain(|item| item.id != project.id);
        projects.push(project.clone());
        Ok(())
    }

    async fn load(
        &self,
        project_id: &str,
    ) -> Result<Option<PresentationProject>, PresentationStoreError> {
        Ok(self
            .projects
            .lock()
            .unwrap()
            .iter()
            .find(|item| item.id == project_id)
            .cloned())
    }

    async fn list(
        &self,
        owner_id: &str,
    ) -> Result<Vec<PresentationProject>, PresentationStoreError> {
        Ok(self
            .projects
            .lock()
            .unwrap()
            .iter()
            .filter(|item| item.owner_id == owner_id)
            .cloned()
            .collect())
    }
}

struct UnusedExporter;

#[async_trait]
impl PresentationExporter for UnusedExporter {
    async fn export(
        &self,
        _: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError> {
        Err(PresentationExportError::Failed(anyhow::anyhow!(
            "unused in dashboard"
        )))
    }
}

async fn seed_sqlite_user(pool: &SqlitePool, actor: &Actor) {
    sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, 'hash', 'user', '2026-01-01', '2026-01-01')")
        .bind(&actor.id.0)
        .bind(format!("user-{}", actor.id.0))
        .execute(pool)
        .await
        .unwrap();
}

async fn seed_sqlite_notification(
    pool: &SqlitePool,
    id: &str,
    actor: &Actor,
    is_read: bool,
    created_at: &str,
) {
    sqlx::query("INSERT INTO notifications (id, user_id, type, title, content, is_read, link, created_at) VALUES (?, ?, 'system', ?, 'content', ?, '/target', ?)")
        .bind(id)
        .bind(&actor.id.0)
        .bind(format!("title-{id}"))
        .bind(if is_read { 1_i64 } else { 0_i64 })
        .bind(created_at)
        .execute(pool)
        .await
        .unwrap();
}

fn sqlite_url(label: &str) -> (std::path::PathBuf, String) {
    let path = std::env::temp_dir().join(format!("revue-{label}-{}.db", uuid::Uuid::new_v4()));
    (
        path.clone(),
        format!("sqlite://{}?mode=rwc", path.display()),
    )
}

#[tokio::test]
async fn sqlite_notifications_preserve_order_pagination_owner_scope_and_restart() {
    let (path, url) = sqlite_url("notifications");
    let repository = Arc::new(
        SqliteNotificationRepository::connect(&url, 2)
            .await
            .unwrap(),
    );
    let service = NotificationApplicationService::new(repository);
    let pool = SqlitePool::connect(&url).await.unwrap();
    let owner = Actor::user("11111111-1111-1111-1111-111111111111");
    let other = Actor::user("22222222-2222-2222-2222-222222222222");
    seed_sqlite_user(&pool, &owner).await;
    seed_sqlite_user(&pool, &other).await;
    seed_sqlite_notification(&pool, "n1", &owner, false, "2026-01-01T00:00:00Z").await;
    seed_sqlite_notification(&pool, "n2", &owner, true, "2026-01-02T00:00:00Z").await;
    seed_sqlite_notification(&pool, "n3", &owner, false, "2026-01-03T00:00:00Z").await;
    seed_sqlite_notification(&pool, "other", &other, false, "2026-01-04T00:00:00Z").await;

    let first = service.list(&owner, false, Some(1), Some(2)).await.unwrap();
    assert_eq!(
        first
            .iter()
            .map(|item| item.id.as_str())
            .collect::<Vec<_>>(),
        vec!["n3", "n2"]
    );
    assert_eq!(
        service.list(&owner, false, Some(2), Some(2)).await.unwrap()[0].id,
        "n1"
    );
    assert_eq!(
        service.list(&owner, true, None, None).await.unwrap().len(),
        2
    );
    assert_eq!(service.unread_count(&owner).await.unwrap(), 2);
    assert!(!service.mark_read(&owner, "other".into()).await.unwrap());
    assert!(!service.delete(&owner, "other".into()).await.unwrap());
    assert!(service.mark_read(&owner, "n3".into()).await.unwrap());
    service.mark_all_read(&owner).await.unwrap();
    assert_eq!(service.unread_count(&owner).await.unwrap(), 0);
    assert!(service.delete(&owner, "n2".into()).await.unwrap());
    drop(service);
    drop(pool);

    let restarted = NotificationApplicationService::new(Arc::new(
        SqliteNotificationRepository::connect(&url, 1)
            .await
            .unwrap(),
    ));
    let rows = restarted.list(&owner, false, None, None).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|item| item.user_id == owner.id.0));
    let json = serde_json::to_value(&rows[0]).unwrap();
    assert_eq!(json["type"], "system");
    assert!(json.get("notification_type").is_none());
    drop(restarted);
    let _ = tokio::fs::remove_file(path).await;
}

async fn build_dashboard(
    url: &str,
    notifications: Arc<NotificationApplicationService>,
) -> (
    Arc<DashboardApplicationService>,
    Arc<ProjectApplicationService>,
    Arc<SessionApplicationService>,
) {
    let sessions_repository = Arc::new(SqliteSessionRepository::connect(url, 2).await.unwrap());
    let project_repository = Arc::new(SqliteProjectRepository::connect(url, 2).await.unwrap());
    let asset_repository = Arc::new(SqliteAssetRepository::connect(url, 2).await.unwrap());
    let presentation = Arc::new(PresentationCapability::new(
        Arc::new(UnusedPlanner),
        Arc::new(MemoryPresentationStore::default()),
        Arc::new(UnusedExporter),
    ));
    let projects = Arc::new(ProjectApplicationService::new(
        project_repository,
        sessions_repository.clone(),
        presentation,
    ));
    let sessions = Arc::new(SessionApplicationService::new(sessions_repository));
    let assets = Arc::new(AssetApplicationService::new(
        asset_repository,
        Arc::new(UnusedStorage),
        Arc::new(UnusedExtractor),
    ));
    let dashboard = Arc::new(DashboardApplicationService::new(
        projects.clone(),
        sessions.clone(),
        assets,
        notifications,
    ));
    (dashboard, projects, sessions)
}

#[tokio::test]
async fn dashboard_aggregates_application_facades_with_compatible_json_shape() {
    let (path, url) = sqlite_url("dashboard");
    let notification_repository = Arc::new(
        SqliteNotificationRepository::connect(&url, 2)
            .await
            .unwrap(),
    );
    let notifications = Arc::new(NotificationApplicationService::new(notification_repository));
    let (dashboard, projects, sessions) = build_dashboard(&url, notifications).await;
    let actor = Actor::user("33333333-3333-3333-3333-333333333333");
    let pool = SqlitePool::connect(&url).await.unwrap();
    seed_sqlite_user(&pool, &actor).await;

    projects
        .create(
            &actor,
            "Document".into(),
            Some("desc".into()),
            Some("doc".into()),
        )
        .await
        .unwrap();
    projects
        .create(&actor, "Sheet".into(), None, Some("sheet".into()))
        .await
        .unwrap();
    projects
        .create_presentation(&actor, "Deck".into(), None)
        .await
        .unwrap();
    for index in 0..7 {
        sessions
            .create(
                &actor,
                NewConversation {
                    owner_id: "ignored".into(),
                    project_id: None,
                    tool_kind: Some("general".into()),
                    title: format!("Session {index}"),
                },
            )
            .await
            .unwrap();
    }
    sqlx::query("INSERT INTO files (id, owner_id, name, file_path, file_type, file_size, created_at, updated_at) VALUES ('f1', ?, 'a.pdf', '/tmp/a', 'pdf', 10, '2026-01-01', '2026-01-01'), ('f2', ?, 'b.pdf', '/tmp/b', 'pdf', 20, '2026-01-02', '2026-01-02'), ('f3', ?, 'c.docx', '/tmp/c', 'docx', 5, '2026-01-03', '2026-01-03')")
        .bind(&actor.id.0).bind(&actor.id.0).bind(&actor.id.0).execute(&pool).await.unwrap();
    seed_sqlite_notification(&pool, "dashboard-n1", &actor, false, "2026-01-01").await;
    seed_sqlite_notification(&pool, "dashboard-n2", &actor, true, "2026-01-02").await;

    let summary = dashboard.summary(&actor).await.unwrap();
    assert_eq!(summary.projects.total, 3);
    assert_eq!(summary.projects.by_kind["doc"], 1);
    assert_eq!(summary.projects.by_kind["sheet"], 1);
    assert_eq!(summary.projects.by_kind["ppt"], 1);
    assert_eq!(summary.files.total, 3);
    assert_eq!(summary.files.total_size, 35);
    assert_eq!(summary.files.by_type["pdf"], 2);
    assert_eq!(summary.notifications.unread, 1);
    assert_eq!(summary.recent_sessions.len(), 5);
    assert_eq!(summary.recent_projects.len(), 2);

    let json = serde_json::to_value(summary).unwrap();
    for key in [
        "projects",
        "files",
        "notifications",
        "recent_sessions",
        "recent_projects",
    ] {
        assert!(json.get(key).is_some(), "missing dashboard key {key}");
    }
    assert_eq!(json["files"]["total"], 3);
    assert_eq!(json["notifications"]["unread"], 1);
    drop(dashboard);
    drop(projects);
    drop(sessions);
    pool.close().await;
    let _ = tokio::fs::remove_file(path).await;
}

struct FailingNotificationRepository;

#[async_trait]
impl NotificationRepository for FailingNotificationRepository {
    async fn list(
        &self,
        _: revue_office_lib::application::notifications::NotificationQuery,
    ) -> Result<Vec<revue_office_lib::application::notifications::Notification>, NotificationError>
    {
        Err(NotificationError::Repository(anyhow::anyhow!("synthetic")))
    }
    async fn unread_count(
        &self,
        _: &revue_office_lib::application::identity::ActorId,
    ) -> Result<i64, NotificationError> {
        Err(NotificationError::Repository(anyhow::anyhow!("synthetic")))
    }
    async fn mark_read(
        &self,
        _: &revue_office_lib::application::identity::ActorId,
        _: &revue_office_lib::application::notifications::NotificationId,
    ) -> Result<bool, NotificationError> {
        unreachable!()
    }
    async fn mark_all_read(
        &self,
        _: &revue_office_lib::application::identity::ActorId,
    ) -> Result<(), NotificationError> {
        unreachable!()
    }
    async fn delete(
        &self,
        _: &revue_office_lib::application::identity::ActorId,
        _: &revue_office_lib::application::notifications::NotificationId,
    ) -> Result<bool, NotificationError> {
        unreachable!()
    }
}

#[tokio::test]
async fn dashboard_propagates_partial_dependency_failure_instead_of_fabricating_zeroes() {
    let (path, url) = sqlite_url("dashboard-failure");
    let notifications = Arc::new(NotificationApplicationService::new(Arc::new(
        FailingNotificationRepository,
    )));
    let (dashboard, _, _) = build_dashboard(&url, notifications).await;
    let actor = Actor::user("44444444-4444-4444-4444-444444444444");
    assert!(matches!(
        dashboard.summary(&actor).await.unwrap_err(),
        DashboardError::Notifications(_)
    ));
    drop(dashboard);
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
#[ignore = "requires REVUE_ALLOW_MYSQL_TEST=1 and isolated MYSQL_TEST_DATABASE_URL"]
async fn mysql_notifications_use_the_same_owner_scoped_contract() {
    assert_eq!(std::env::var("REVUE_ALLOW_MYSQL_TEST").as_deref(), Ok("1"));
    let url = std::env::var("MYSQL_TEST_DATABASE_URL").unwrap();
    let database = url
        .split('?')
        .next()
        .unwrap_or(&url)
        .rsplit('/')
        .next()
        .unwrap_or("");
    assert!(
        url.starts_with("mysql://")
            && (database.starts_with("test_") || database.ends_with("_test"))
    );

    let repository = Arc::new(MySqlNotificationRepository::connect(&url, 2).await.unwrap());
    let service = NotificationApplicationService::new(repository);
    let pool = MySqlPool::connect(&url).await.unwrap();
    let owner = Actor::user("55555555-5555-5555-5555-555555555555");
    let other = Actor::user("66666666-6666-6666-6666-666666666666");
    for actor in [&owner, &other] {
        sqlx::query("INSERT INTO users (id, username, password_hash, role, created_at, updated_at) VALUES (?, ?, 'hash', 'user', '2026-01-01', '2026-01-01')")
            .bind(&actor.id.0).bind(format!("user-{}", actor.id.0)).execute(&pool).await.unwrap();
    }
    for (id, actor, read, created) in [
        ("mysql-n1", &owner, 0_i8, "2026-01-01"),
        ("mysql-n2", &owner, 1_i8, "2026-01-02"),
        ("mysql-other", &other, 0_i8, "2026-01-03"),
    ] {
        sqlx::query("INSERT INTO notifications (id, user_id, type, title, is_read, created_at) VALUES (?, ?, 'system', ?, ?, ?)")
            .bind(id).bind(&actor.id.0).bind(id).bind(read).bind(created).execute(&pool).await.unwrap();
    }

    let rows = service.list(&owner, false, None, None).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].id, "mysql-n2");
    assert_eq!(service.unread_count(&owner).await.unwrap(), 1);
    assert!(
        !service
            .mark_read(&owner, "mysql-other".into())
            .await
            .unwrap()
    );
    assert!(!service.delete(&owner, "mysql-other".into()).await.unwrap());
    assert!(service.mark_read(&owner, "mysql-n1".into()).await.unwrap());
    service.mark_all_read(&owner).await.unwrap();
    assert_eq!(service.unread_count(&owner).await.unwrap(), 0);
    assert!(service.delete(&owner, "mysql-n2".into()).await.unwrap());
}
