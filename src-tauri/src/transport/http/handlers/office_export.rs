use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{StatusCode, header};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};

use crate::application::office_export::{
    DocumentExportRequest, ExportedFile, OfficeExportApplicationService, OfficeExportError,
    SpreadsheetExportRequest,
};
use crate::transport::http::HttpState;
use crate::transport::http::error::AppError;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/doc/export", post(export_document))
        .route("/api/excel/export", post(export_spreadsheet))
        .route("/api/files/download/:filename", get(download_file))
}

async fn export_document(
    State(service): State<Arc<OfficeExportApplicationService>>,
    Json(request): Json<DocumentExportRequest>,
) -> Result<Response, AppError> {
    response(service.export_document(request).await.map_err(map_error)?)
}

async fn export_spreadsheet(
    State(service): State<Arc<OfficeExportApplicationService>>,
    Json(request): Json<SpreadsheetExportRequest>,
) -> Result<Response, AppError> {
    response(
        service
            .export_spreadsheet(request)
            .await
            .map_err(map_error)?,
    )
}

async fn download_file(
    State(service): State<Arc<OfficeExportApplicationService>>,
    Path(filename): Path<String>,
) -> Result<Response, AppError> {
    response(service.download(&filename).await.map_err(map_error)?)
}

fn map_error(error: OfficeExportError) -> AppError {
    match error {
        OfficeExportError::Invalid(message) => AppError::BadRequest(message),
        OfficeExportError::NotFound => AppError::NotFound("文件不存在".into()),
        other => AppError::Internal(anyhow::Error::new(other)),
    }
}

fn response(file: ExportedFile) -> Result<Response, AppError> {
    let encoded = urlencoding::encode(&file.filename);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, file.content_type)
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                file.filename, encoded
            ),
        )
        .body(Body::from(file.bytes))
        .map_err(|error| AppError::Internal(error.into()))
}
