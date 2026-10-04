use std::sync::Arc;

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Multipart, Path, Query, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use secrecy::SecretString;
use serde::Deserialize;
use serde_json::json;

use crate::application::assets::service::sanitize_filename;
use crate::application::assets::{
    AssetApplicationService, AssetError, AssetId, FolderId, NewAsset,
};
use crate::application::identity::{IdentityAdapter, IdentityError, IdentityProof};
use crate::transport::http::HttpState;
use crate::transport::http::auth::{AuthenticatedActor, IdentityState};
use crate::transport::http::error::AppError;

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

pub fn router() -> Router<HttpState> {
    Router::new()
        .route("/api/files", get(list_files))
        .route("/api/files/search", get(search_files))
        .route("/api/files/stats", get(file_stats))
        .route("/api/files/extract", post(extract_file_text))
        .route("/api/files/upload", post(upload_file))
        .route("/api/files/:id/content", get(get_file_content))
        .route("/api/files/:id/thumbnail", get(get_file_thumbnail))
        .route("/api/files/:id/preview", get(get_file_preview))
        .route("/api/files/:id/stream", get(stream_file))
        .route("/api/files/:id", get(get_file).delete(delete_file))
        .route("/api/files/:id/download", get(download_file))
        .route("/api/files/folders/list", get(list_folders))
        .route("/api/folders", post(create_folder))
        .route("/api/folders/:id", delete(delete_folder))
        .layer(DefaultBodyLimit::max(MAX_UPLOAD_BYTES))
}

#[derive(Deserialize)]
struct FileQuery {
    #[serde(default)]
    folder_id: Option<String>,
    #[serde(default)]
    q: Option<String>,
}
#[derive(Deserialize)]
struct FolderQuery {
    #[serde(default)]
    parent_id: Option<String>,
}
#[derive(Deserialize)]
struct StreamQuery {
    #[serde(default)]
    token: Option<String>,
}
#[derive(Deserialize)]
struct CreateFolderReq {
    name: String,
    #[serde(default)]
    parent_id: Option<String>,
}

fn map_error(error: AssetError) -> AppError {
    match error {
        AssetError::Invalid(message) => AppError::BadRequest(message),
        AssetError::NotFound => AppError::NotFound("文件不存在".into()),
        AssetError::Forbidden => AppError::Forbidden,
        other => AppError::Internal(anyhow::Error::new(other)),
    }
}

async fn list_files(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Query(q): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let files = service
        .list(&actor, q.folder_id.map(FolderId))
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "files": files })))
}
async fn search_files(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Query(q): Query<FileQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let files = service.search(&actor, q.q).await.map_err(map_error)?;
    Ok(Json(json!({ "files": files })))
}
async fn file_stats(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
) -> Result<Json<serde_json::Value>, AppError> {
    let stats = service.stats(&actor).await.map_err(map_error)?;
    Ok(Json(
        json!({ "by_type": stats.by_type, "total_size": stats.total_size, "total_files": stats.total_files, "total": stats.total_files, "size": stats.total_size }),
    ))
}

async fn multipart_file(
    headers: &HeaderMap,
    multipart: &mut Multipart,
) -> Result<(String, String, Vec<u8>), AppError> {
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| AppError::BadRequest(format!("上传表单解析失败: {e}")))?
    {
        if field.name() != Some("file") {
            continue;
        }
        let name = field
            .file_name()
            .map(str::to_owned)
            .or_else(|| header_value(headers, "x-filename"))
            .unwrap_or_else(|| "upload.bin".into());
        let mime = field.content_type().map(str::to_owned).unwrap_or_else(|| {
            mime_guess::from_path(&name)
                .first_or_octet_stream()
                .to_string()
        });
        let bytes = field
            .bytes()
            .await
            .map_err(|e| AppError::BadRequest(format!("读取上传文件失败: {e}")))?
            .to_vec();
        return Ok((name, mime, bytes));
    }
    Err(AppError::BadRequest("没有找到 file 字段".into()))
}

async fn upload_file(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let folder_id = header_value(&headers, "x-folder-id").map(FolderId);
    let description = header_value(&headers, "x-description");
    let (name, mime_type, bytes) = multipart_file(&headers, &mut multipart).await?;
    let file = service
        .upload(
            &actor,
            NewAsset {
                name,
                mime_type,
                bytes,
                folder_id,
                description,
            },
        )
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "ok": true, "file": file })))
}
async fn extract_file_text(
    State(service): State<Arc<AssetApplicationService>>,
    _actor: AuthenticatedActor,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<Json<serde_json::Value>, AppError> {
    let (name, mime_type, bytes) = multipart_file(&headers, &mut multipart).await?;
    let name = sanitize_filename(&name);
    let size = bytes.len();
    let extracted = service
        .extract_upload(name.clone(), mime_type.clone(), bytes)
        .await
        .map_err(map_error)?;
    Ok(Json(
        json!({ "ok": !extracted.text.trim().is_empty() && extracted.parser != "unsupported", "name": name, "mime_type": mime_type, "size": size, "parser": extracted.parser, "truncated": extracted.truncated, "text": extracted.text }),
    ))
}
async fn get_file(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    Ok(Json(json!(
        service.get(&actor, AssetId(id)).await.map_err(map_error)?
    )))
}
async fn download_file(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let content = service.read(&actor, AssetId(id)).await.map_err(map_error)?;
    download_response(content.bytes, &content.asset.name)
}
async fn stream_file(
    State(service): State<Arc<AssetApplicationService>>,
    Path(id): Path<String>,
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
    State(identity): State<IdentityState>,
) -> Result<Response, AppError> {
    let token = q
        .token
        .or_else(|| {
            headers
                .get(header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "))
                .map(str::to_owned)
        })
        .ok_or(AppError::Unauthorized)?;
    let actor = authenticate(identity.0.as_ref(), token).await?;
    let content = service.read(&actor, AssetId(id)).await.map_err(map_error)?;
    let mime = mime_guess::from_path(&content.asset.name).first_or_octet_stream();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(
            header::CONTENT_DISPOSITION,
            format!("inline; filename=\"{}\"", content.asset.name),
        )
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .header(header::ACCEPT_RANGES, "bytes")
        .body(Body::from(content.bytes))
        .map_err(|e| AppError::Internal(e.into()))
}
async fn authenticate(
    identity: &dyn IdentityAdapter,
    token: String,
) -> Result<crate::application::identity::Actor, AppError> {
    identity
        .authenticate(IdentityProof(SecretString::new(token)))
        .await
        .map_err(|e| match e {
            IdentityError::Forbidden => AppError::Forbidden,
            _ => AppError::Unauthorized,
        })
}
async fn get_file_content(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let (file, value) = service
        .text_content(&actor, AssetId(id))
        .await
        .map_err(map_error)?;
    Ok(Json(
        json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "text": value.text, "parser": value.parser, "truncated": value.truncated }),
    ))
}
async fn delete_file(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = service
        .delete(&actor, AssetId(id.clone()))
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "deleted": deleted, "id": id })))
}
async fn get_file_thumbnail(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let file = service
        .get(&actor, AssetId(id.clone()))
        .await
        .map_err(map_error)?;
    if file.file_type == "image" {
        let content = service.read(&actor, AssetId(id)).await.map_err(map_error)?;
        let mime = mime_guess::from_path(&file.name).first_or_octet_stream();
        return Response::builder()
            .status(StatusCode::OK)
            .header(header::CONTENT_TYPE, mime.as_ref())
            .header(header::CACHE_CONTROL, "public, max-age=3600")
            .body(Body::from(content.bytes))
            .map_err(|e| AppError::Internal(e.into()));
    }
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            json!({ "file_type": file.file_type, "name": file.name, "file_size": file.file_size })
                .to_string(),
        ))
        .map_err(|e| AppError::Internal(e.into()))
}
async fn get_file_preview(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let file = service
        .get(&actor, AssetId(id.clone()))
        .await
        .map_err(map_error)?;
    let ext = std::path::Path::new(&file.name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    let mime = file
        .metadata
        .as_ref()
        .and_then(|m| m.get("mime_type"))
        .and_then(|v| v.as_str())
        .map(str::to_owned)
        .unwrap_or_else(|| {
            mime_guess::from_path(&file.name)
                .first_or_octet_stream()
                .to_string()
        });
    if file.file_type == "video"
        || matches!(
            ext.as_str(),
            "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv" | "wmv" | "m4v" | "3gp" | "ogv"
        )
    {
        return Ok(Json(
            json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "preview_type": "video", "mime_type": mime, "video_url": format!("/api/files/{}/stream", file.id), "file_size": file.file_size }),
        ));
    }
    let content = service.read(&actor, AssetId(id)).await.map_err(map_error)?;
    if file.file_type == "image"
        || matches!(
            ext.as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg"
        )
    {
        return Ok(Json(
            json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "preview_type": "image", "mime_type": mime, "data_url": format!("data:{};base64,{}", mime, base64_encode(&content.bytes)), "file_size": file.file_size }),
        ));
    }
    if ext == "drawio" || ext == "xml" || matches!(ext.as_str(), "md" | "markdown" | "txt") {
        let text = String::from_utf8_lossy(&content.bytes);
        let preview_type = if ext == "drawio" || ext == "xml" {
            "drawio"
        } else {
            "markdown"
        };
        return Ok(Json(
            json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "preview_type": preview_type, "text": text, "file_size": file.file_size }),
        ));
    }
    let (file, value) = service
        .structured_preview(&actor, AssetId(file.id.clone()))
        .await
        .map_err(map_error)?;
    let preview_type = match ext.as_str() {
        "xlsx" | "xls" | "csv" | "tsv" => "spreadsheet",
        "docx" | "doc" => "document",
        "pptx" | "ppt" => "presentation",
        "pdf" => "pdf",
        _ => "text",
    };
    if matches!(
        value.preview_type.as_str(),
        "presentation" | "spreadsheet" | "document"
    ) {
        Ok(Json(
            json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "preview_type": value.preview_type, "structured": value.data, "parser": value.parser, "truncated": value.truncated, "file_size": file.file_size }),
        ))
    } else {
        Ok(Json(
            json!({ "id": file.id, "name": file.name, "file_type": file.file_type, "preview_type": preview_type, "text": value.data.get("text").and_then(|v| v.as_str()).unwrap_or(""), "parser": value.parser, "truncated": value.truncated, "file_size": file.file_size }),
        ))
    }
}
async fn list_folders(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Query(q): Query<FolderQuery>,
) -> Result<Json<serde_json::Value>, AppError> {
    let folders = service
        .list_folders(&actor, q.parent_id.map(FolderId))
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "folders": folders })))
}
async fn create_folder(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Json(payload): Json<CreateFolderReq>,
) -> Result<Json<serde_json::Value>, AppError> {
    let folder = service
        .create_folder(&actor, payload.name, payload.parent_id.map(FolderId))
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "ok": true, "folder": folder })))
}
async fn delete_folder(
    State(service): State<Arc<AssetApplicationService>>,
    AuthenticatedActor(actor): AuthenticatedActor,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let deleted = service
        .delete_folder(&actor, FolderId(id.clone()))
        .await
        .map_err(map_error)?;
    Ok(Json(json!({ "deleted": deleted, "id": id })))
}
fn header_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get(name)
        .and_then(|v| v.to_str().ok())
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
}
fn download_response(bytes: Vec<u8>, filename: &str) -> Result<Response, AppError> {
    let mime = mime_guess::from_path(filename).first_or_octet_stream();
    let encoded = urlencoding::encode(filename);
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime.as_ref())
        .header(
            header::CONTENT_DISPOSITION,
            format!(
                "attachment; filename=\"{}\"; filename*=UTF-8''{}",
                filename, encoded
            ),
        )
        .body(Body::from(bytes))
        .map_err(|e| AppError::Internal(e.into()))
}
fn base64_encode(data: &[u8]) -> String {
    const C: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        out.push(C[(b[0] >> 2) as usize] as char);
        out.push(C[((b[0] & 3) << 4 | b[1] >> 4) as usize] as char);
        if chunk.len() > 1 {
            out.push(C[((b[1] & 15) << 2 | b[2] >> 6) as usize] as char)
        } else {
            out.push('=')
        };
        if chunk.len() > 2 {
            out.push(C[(b[2] & 63) as usize] as char)
        } else {
            out.push('=')
        }
    }
    out
}
