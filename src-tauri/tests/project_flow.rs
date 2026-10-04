use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use revue_office_lib::application::conversations::SessionRepository;
use revue_office_lib::application::conversations::model::NewConversation;
use revue_office_lib::application::identity::{Actor, ActorId};
use revue_office_lib::application::projects::{
    NewProject, ProjectListQuery, ProjectRepository, ProjectUpdate,
};
use revue_office_lib::capabilities::presentation::{
    PresentationCapability, PresentationExport, PresentationExportError, PresentationExporter,
    PresentationPlan, PresentationPlanRequest, PresentationPlanner, PresentationPlannerError,
    PresentationProject,
};
use revue_office_lib::infrastructure::persistence::mysql::projects::MySqlProjectRepository;
use revue_office_lib::infrastructure::persistence::sqlite::SqliteSessionRepository;
use revue_office_lib::infrastructure::persistence::sqlite::projects::SqliteProjectRepository;
use revue_office_lib::infrastructure::presentation_store::local::LocalPresentationStore;

struct UnusedPlanner;

#[async_trait]
impl PresentationPlanner for UnusedPlanner {
    async fn plan(
        &self,
        _request: PresentationPlanRequest,
    ) -> Result<PresentationPlan, PresentationPlannerError> {
        Err(PresentationPlannerError::InvalidResponse(
            "planner must not run during project CRUD".into(),
        ))
    }
}

struct FakeExporter;

#[async_trait]
impl PresentationExporter for FakeExporter {
    async fn export(
        &self,
        _project: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError> {
        Ok(PresentationExport {
            format: "pptx".into(),
            bytes: b"compatible-pptx".to_vec(),
        })
    }
}

fn sqlite_url() -> (PathBuf, String) {
    let path = std::env::temp_dir().join(format!("revue-projects-{}.db", uuid::Uuid::new_v4()));
    (
        path.clone(),
        format!("sqlite://{}?mode=rwc", path.display()),
    )
}

async fn seed_sqlite_user(url: &str, owner: &str) {
    let pool = sqlx::SqlitePool::connect(url).await.unwrap();
    let now = "2026-10-02T00:00:00Z";
    sqlx::query("INSERT OR IGNORE INTO users (id, username, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)")
        .bind(owner)
        .bind(format!("user-{owner}"))
        .bind("test-only")
        .bind(now)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();
    pool.close().await;
}

async fn exercise_repository(repository: &dyn ProjectRepository, owner: &str) {
    let owner_id = ActorId(owner.into());
    let created = repository
        .create(NewProject {
            owner_id: owner_id.clone(),
            title: "Quarterly plan".into(),
            description: Some("Initial".into()),
            tool_kind: "presentation".into(),
        })
        .await
        .unwrap();
    assert_eq!(created.owner_id, owner);
    assert_eq!(
        repository
            .find(
                &owner_id,
                &revue_office_lib::application::projects::ProjectId(created.id.clone())
            )
            .await
            .unwrap()
            .unwrap()
            .description
            .as_deref(),
        Some("Initial")
    );
    assert_eq!(
        repository
            .list(ProjectListQuery {
                owner_id: owner_id.clone(),
                query: Some("Quarterly".into()),
            })
            .await
            .unwrap()
            .len(),
        1
    );
    assert!(
        repository
            .list(ProjectListQuery {
                owner_id: ActorId("another-owner".into()),
                query: None,
            })
            .await
            .unwrap()
            .is_empty()
    );
    let updated = repository
        .update(ProjectUpdate {
            owner_id: owner_id.clone(),
            id: revue_office_lib::application::projects::ProjectId(created.id.clone()),
            title: Some("Updated plan".into()),
            description: Some(None),
            tool_kind: None,
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(updated.title, "Updated plan");
    assert_eq!(updated.description, None);
    assert!(
        repository
            .delete(
                &owner_id,
                &revue_office_lib::application::projects::ProjectId(created.id)
            )
            .await
            .unwrap()
    );
}

#[tokio::test]
async fn sqlite_project_repository_survives_reconnect() {
    let (path, url) = sqlite_url();
    let repository = SqliteProjectRepository::connect(&url, 1).await.unwrap();
    seed_sqlite_user(&url, "sqlite-owner").await;
    exercise_repository(&repository, "sqlite-owner").await;

    let kept = repository
        .create(NewProject {
            owner_id: ActorId("sqlite-owner".into()),
            title: "Persisted".into(),
            description: None,
            tool_kind: "general".into(),
        })
        .await
        .unwrap();
    drop(repository);
    let reopened = SqliteProjectRepository::connect(&url, 1).await.unwrap();
    assert!(
        reopened
            .find(
                &ActorId("sqlite-owner".into()),
                &revue_office_lib::application::projects::ProjectId(kept.id)
            )
            .await
            .unwrap()
            .is_some()
    );
    drop(reopened);
    let _ = tokio::fs::remove_file(path).await;
}

#[tokio::test]
async fn presentation_projects_are_owner_scoped_and_restart_readable() {
    let root = std::env::temp_dir().join(format!("revue-ppt-projects-{}", uuid::Uuid::new_v4()));
    let (_db_path, database_url) = sqlite_url();
    let sessions = Arc::new(
        SqliteSessionRepository::connect(&database_url, 1)
            .await
            .unwrap(),
    );
    let projects = Arc::new(
        SqliteProjectRepository::connect(&database_url, 1)
            .await
            .unwrap(),
    );
    let capability = Arc::new(PresentationCapability::new(
        Arc::new(UnusedPlanner),
        Arc::new(LocalPresentationStore::new(&root)),
        Arc::new(FakeExporter),
    ));
    let service = revue_office_lib::application::projects::ProjectApplicationService::new(
        projects,
        sessions.clone(),
        capability,
    );
    let actor = Actor::user("owner-one");
    let created = service
        .create_presentation(&actor, "Editable deck".into(), Some("tech".into()))
        .await
        .unwrap();
    let updated = service
        .update_presentation(&actor, &created.id, Some("Renamed deck".into()), None)
        .await
        .unwrap();
    assert_eq!(updated.title, "Renamed deck");
    assert!(matches!(
        service
            .get_presentation(&Actor::user("owner-two"), &created.id)
            .await,
        Err(revue_office_lib::application::projects::ProjectError::Forbidden)
    ));

    let restarted_capability = Arc::new(PresentationCapability::new(
        Arc::new(UnusedPlanner),
        Arc::new(LocalPresentationStore::new(&root)),
        Arc::new(FakeExporter),
    ));
    let restarted = revue_office_lib::application::projects::ProjectApplicationService::new(
        Arc::new(
            SqliteProjectRepository::connect(&database_url, 1)
                .await
                .unwrap(),
        ),
        sessions,
        restarted_capability,
    );
    assert_eq!(
        restarted
            .get_presentation(&actor, &created.id)
            .await
            .unwrap()
            .title,
        "Renamed deck"
    );
    assert_eq!(restarted.list_presentations(&actor).await.unwrap().len(), 1);
    assert!(
        restarted
            .delete_presentation(&actor, &created.id)
            .await
            .unwrap()
    );
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn historical_presentation_json_defaults_optional_fields_and_preserves_endpoint_shape() {
    let root = std::env::temp_dir().join(format!("revue-ppt-history-{}", uuid::Uuid::new_v4()));
    tokio::fs::create_dir_all(&root).await.unwrap();
    tokio::fs::write(
        root.join("historical.json"),
        r#"{"id":"historical","title":"Old","theme":"default","slides":[],"created_at":"2024-01-01","updated_at":"2024-01-01","owner_id":"owner","unknown":"preserved-on-read"}"#,
    )
    .await
    .unwrap();
    let store = Arc::new(LocalPresentationStore::new(&root));
    let capability =
        PresentationCapability::new(Arc::new(UnusedPlanner), store, Arc::new(FakeExporter));
    let project = capability.get_project("historical").await.unwrap().unwrap();
    assert!(project.history.is_empty());
    assert_eq!(project.layout, "16x9");

    let source = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("src/transport/http/handlers/projects.rs"),
    )
    .unwrap();
    for route in [
        "/api/projects",
        "/api/projects/:project_id",
        "/api/projects/:project_id/sessions",
        "/api/ppt/projects",
        "/api/ppt/project/:project_id",
        "/api/ppt/project/:project_id/slides",
        "/api/ppt/project/:project_id/export",
        "/api/ppt/project",
        "/api/ppt/project/:project_id/delete",
    ] {
        assert!(source.contains(route), "missing compatible route {route}");
    }
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn project_sessions_use_injected_session_repository() {
    let (path, url) = sqlite_url();
    let sessions = Arc::new(SqliteSessionRepository::connect(&url, 1).await.unwrap());
    let projects = Arc::new(SqliteProjectRepository::connect(&url, 1).await.unwrap());
    seed_sqlite_user(&url, "session-owner").await;
    let root = std::env::temp_dir().join(format!("revue-project-session-{}", uuid::Uuid::new_v4()));
    let service = revue_office_lib::application::projects::ProjectApplicationService::new(
        projects,
        sessions.clone(),
        Arc::new(PresentationCapability::new(
            Arc::new(UnusedPlanner),
            Arc::new(LocalPresentationStore::new(&root)),
            Arc::new(FakeExporter),
        )),
    );
    let actor = Actor::user("session-owner");
    let project = service
        .create(&actor, "Project".into(), None, None)
        .await
        .unwrap();
    sessions
        .create(NewConversation {
            owner_id: actor.id.0.clone(),
            project_id: Some(project.id.clone()),
            tool_kind: Some("presentation".into()),
            title: "Inside".into(),
        })
        .await
        .unwrap();
    sessions
        .create(NewConversation {
            owner_id: actor.id.0.clone(),
            project_id: None,
            tool_kind: None,
            title: "Outside".into(),
        })
        .await
        .unwrap();
    let result = service.sessions(&actor, project.id).await.unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].title, "Inside");
    let _ = tokio::fs::remove_file(path).await;
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
#[ignore = "requires REVUE_ALLOW_MYSQL_TEST=1 and isolated MYSQL_TEST_DATABASE_URL"]
async fn mysql_project_contract_requires_isolated_database() {
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
    let repository = MySqlProjectRepository::connect(&url, 1).await.unwrap();
    exercise_repository(&repository, &uuid::Uuid::new_v4().to_string()).await;
}
