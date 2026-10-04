use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::json;

use crate::application::projects::{ProjectApplicationService, ProjectError};
use crate::transport::http::HttpState;
use crate::transport::http::auth::AuthenticatedActor;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/projects", get(list_projects).post(create_project))
        .route(
            "/api/projects/:project_id",
            get(get_project)
                .patch(update_project)
                .delete(delete_project),
        )
        .route(
            "/api/projects/:project_id/sessions",
            get(get_project_sessions),
        )
        .route("/api/ppt/projects", get(list_ppt_projects))
        .route(
            "/api/ppt/project/:project_id",
            get(get_ppt_project).patch(update_ppt_project),
        )
        .route("/api/ppt/project/:project_id/slides", get(get_ppt_slides))
        .route(
            "/api/ppt/project/:project_id/export",
            post(export_ppt_project),
        )
        .route("/api/ppt/project", post(create_ppt_project))
        .route(
            "/api/ppt/project/:project_id/delete",
            post(delete_ppt_project),
        )
}

fn map_error(error: ProjectError) -> AppError {
    match error {
        ProjectError::Invalid(message) => AppError::BadRequest(message),
        ProjectError::NotFound => AppError::NotFound("项目不存在".into()),
        ProjectError::Forbidden => AppError::Forbidden,
        other => AppError::Internal(anyhow::Error::new(other)),
    }
}

#[derive(Deserialize)]
struct ProjectListQuery {
    q: Option<String>,
}

async fn list_projects(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Query(query): Query<ProjectListQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let projects = service.list(&actor, query.q).await.map_err(map_error)?;
    Ok(Json(json!({ "projects": projects })))
}

#[derive(Deserialize)]
struct CreateGenericProjectReq {
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tool_kind: Option<String>,
}

async fn create_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Json(request): Json<CreateGenericProjectReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project = service
        .create(
            &actor,
            request.title,
            request.description,
            request.tool_kind,
        )
        .await
        .map_err(map_error)?;
    Ok(Json(json!(project)))
}

async fn get_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(
        service.get(&actor, project_id).await.map_err(map_error)?
    )))
}

#[derive(Deserialize)]
struct UpdateProjectReq {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    tool_kind: Option<String>,
}

async fn update_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
    Json(request): Json<UpdateProjectReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project = service
        .update(
            &actor,
            project_id,
            request.title,
            Some(request.description),
            request.tool_kind,
        )
        .await
        .map_err(map_error)?;
    Ok(Json(json!(project)))
}

async fn delete_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = service
        .delete(&actor, project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn get_project_sessions(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let sessions = service
        .sessions(&actor, project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "sessions": sessions })))
}

async fn list_ppt_projects(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<serde_json::Value>, AppError> {
    let projects = service
        .list_presentations(&actor)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "projects": projects })))
}

async fn get_ppt_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(
        service
            .get_presentation(&actor, &project_id)
            .await
            .map_err(map_error)?
    )))
}

#[derive(Deserialize)]
struct UpdatePptProjectReq {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    theme: Option<String>,
}

async fn update_ppt_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
    Json(request): Json<UpdatePptProjectReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(
        service
            .update_presentation(&actor, &project_id, request.title, request.theme)
            .await
            .map_err(map_error)?
    )))
}

async fn get_ppt_slides(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let project = service
        .get_presentation(&actor, &project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "slides": project.slides })))
}

#[derive(Deserialize)]
struct CreatePptProjectReq {
    title: String,
    #[serde(default)]
    theme: Option<String>,
}

async fn create_ppt_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Json(request): Json<CreatePptProjectReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(
        service
            .create_presentation(&actor, request.title, request.theme)
            .await
            .map_err(map_error)?
    )))
}

async fn delete_ppt_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = service
        .delete_presentation(&actor, &project_id)
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "deleted": deleted })))
}

async fn export_ppt_project(
    State(service): State<Arc<ProjectApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(project_id): Path<String>,
) -> Result<Response, AppError> {
    let (project, export) = service
        .export_presentation(&actor, &project_id)
        .await
        .map_err(map_error)?;
    let filename = format!("{}.pptx", sanitize_filename(&project.title));
    let encoded = urlencoding::encode(&filename);
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        )
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                filename, encoded
            ),
        )
        .body(Body::from(export.bytes))
        .map_err(|error| AppError::Internal(error.into()))
}

fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|character| {
            !matches!(
                character,
                '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
            )
        })
        .collect();
    if cleaned.trim().is_empty() {
        "presentation".into()
    } else {
        cleaned
    }
}
