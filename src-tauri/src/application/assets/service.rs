use std::path::Path;
use std::sync::Arc;

use serde_json::json;

use crate::application::identity::{Actor, ActorId};

use super::error::AssetError;
use super::model::*;
use super::ports::{AssetContentExtractor, AssetRepository, AssetStorage};

const MAX_UPLOAD_BYTES: usize = 50 * 1024 * 1024;

pub struct AssetApplicationService {
    repository: Arc<dyn AssetRepository>,
    storage: Arc<dyn AssetStorage>,
    extractor: Arc<dyn AssetContentExtractor>,
}

impl AssetApplicationService {
    pub fn new(
        repository: Arc<dyn AssetRepository>,
        storage: Arc<dyn AssetStorage>,
        extractor: Arc<dyn AssetContentExtractor>,
    ) -> Self {
        Self {
            repository,
            storage,
            extractor,
        }
    }

    pub async fn upload(&self, actor: &Actor, request: NewAsset) -> Result<Asset, AssetError> {
        validate_bytes(&request.bytes)?;
        let name = sanitize_filename(&request.name);
        if let Some(folder) = request.folder_id.as_ref() {
            self.require_folder(&actor.id, folder).await?;
        }
        let extracted = match self
            .extractor
            .extract_text(AssetContentInput {
                name: name.clone(),
                mime_type: request.mime_type.clone(),
                bytes: request.bytes.clone(),
            })
            .await
        {
            Ok(extracted) => extracted,
            Err(AssetError::Unsupported(_)) => ExtractedText {
                text: String::new(),
                parser: "unsupported".into(),
                truncated: false,
            },
            Err(error) => return Err(error),
        };
        let file_type = infer_file_type(&name, &request.mime_type);
        let size = request.bytes.len() as i64;
        let stored = self
            .storage
            .write(AssetWrite {
                owner_id: actor.id.clone(),
                name: name.clone(),
                bytes: request.bytes,
            })
            .await?;
        let record = NewAssetRecord {
            owner_id: actor.id.clone(),
            name,
            location: stored.location.clone(),
            file_type,
            file_size: size,
            folder_id: request.folder_id,
            description: request.description,
            metadata: Some(json!({
                "mime_type": request.mime_type,
                "text_parser": extracted.parser,
                "text_truncated": extracted.truncated,
                "text_chars": extracted.text.chars().count(),
                "extracted_text": extracted.text,
            })),
        };
        match self.repository.create(record).await {
            Ok(asset) => Ok(asset),
            Err(error) => {
                if let Ok(Some(quarantined)) = self.storage.quarantine(&stored.location).await {
                    let _ = self.storage.purge(quarantined).await;
                }
                Err(error)
            }
        }
    }

    pub async fn extract_upload(
        &self,
        name: String,
        mime_type: String,
        bytes: Vec<u8>,
    ) -> Result<ExtractedText, AssetError> {
        validate_bytes(&bytes)?;
        self.extractor
            .extract_text(AssetContentInput {
                name: sanitize_filename(&name),
                mime_type,
                bytes,
            })
            .await
    }

    pub async fn get(&self, actor: &Actor, id: AssetId) -> Result<Asset, AssetError> {
        self.repository
            .get(&actor.id, &id)
            .await?
            .ok_or(AssetError::NotFound)
    }

    pub async fn list(
        &self,
        actor: &Actor,
        folder_id: Option<FolderId>,
    ) -> Result<Vec<Asset>, AssetError> {
        if let Some(folder) = folder_id.as_ref() {
            self.require_folder(&actor.id, folder).await?;
        }
        self.repository
            .list(AssetListQuery {
                owner_id: actor.id.clone(),
                folder_id,
            })
            .await
    }

    pub async fn search(
        &self,
        actor: &Actor,
        query: Option<String>,
    ) -> Result<Vec<Asset>, AssetError> {
        self.repository
            .search(AssetSearchQuery {
                owner_id: actor.id.clone(),
                query,
            })
            .await
    }

    pub async fn stats(&self, actor: &Actor) -> Result<AssetStats, AssetError> {
        self.repository.stats(&actor.id).await
    }

    pub async fn read(&self, actor: &Actor, id: AssetId) -> Result<AssetContent, AssetError> {
        let asset = self.get(actor, id).await?;
        let bytes = self
            .storage
            .read(&AssetLocation(asset.file_path.clone()))
            .await?;
        Ok(AssetContent { asset, bytes })
    }

    pub async fn text_content(
        &self,
        actor: &Actor,
        id: AssetId,
    ) -> Result<(Asset, ExtractedText), AssetError> {
        let asset = self.get(actor, id).await?;
        if let Some(metadata) = &asset.metadata
            && let Some(text) = metadata.get("extracted_text").and_then(|v| v.as_str())
        {
            return Ok((
                asset.clone(),
                ExtractedText {
                    text: text.into(),
                    parser: metadata
                        .get("text_parser")
                        .and_then(|v| v.as_str())
                        .unwrap_or("stored")
                        .into(),
                    truncated: metadata
                        .get("text_truncated")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false),
                },
            ));
        }
        let bytes = self
            .storage
            .read(&AssetLocation(asset.file_path.clone()))
            .await?;
        let mime_type = asset
            .metadata
            .as_ref()
            .and_then(|v| v.get("mime_type"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                mime_guess::from_path(&asset.name)
                    .first_or_octet_stream()
                    .to_string()
            });
        let extracted = self
            .extractor
            .extract_text(AssetContentInput {
                name: asset.name.clone(),
                mime_type,
                bytes,
            })
            .await?;
        Ok((asset, extracted))
    }

    pub async fn structured_preview(
        &self,
        actor: &Actor,
        id: AssetId,
    ) -> Result<(Asset, StructuredPreview), AssetError> {
        let content = self.read(actor, id).await?;
        let mime_type = content
            .asset
            .metadata
            .as_ref()
            .and_then(|v| v.get("mime_type"))
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .unwrap_or_else(|| {
                mime_guess::from_path(&content.asset.name)
                    .first_or_octet_stream()
                    .to_string()
            });
        let preview = self
            .extractor
            .extract_structured(AssetContentInput {
                name: content.asset.name.clone(),
                mime_type,
                bytes: content.bytes,
            })
            .await?;
        Ok((content.asset, preview))
    }

    pub async fn delete(&self, actor: &Actor, id: AssetId) -> Result<bool, AssetError> {
        let Some(asset) = self.repository.get(&actor.id, &id).await? else {
            return Ok(false);
        };
        let quarantined = self
            .storage
            .quarantine(&AssetLocation(asset.file_path))
            .await?;
        match self.repository.delete(&actor.id, &id).await {
            Ok(deleted) => {
                if deleted {
                    if let Some(item) = quarantined {
                        self.storage.purge(item).await?;
                    }
                } else if let Some(item) = quarantined.as_ref() {
                    self.storage.restore(item).await?;
                }
                Ok(deleted)
            }
            Err(error) => {
                if let Some(item) = quarantined.as_ref() {
                    let _ = self.storage.restore(item).await;
                }
                Err(error)
            }
        }
    }

    pub async fn list_folders(
        &self,
        actor: &Actor,
        parent: Option<FolderId>,
    ) -> Result<Vec<Folder>, AssetError> {
        if let Some(parent) = parent.as_ref() {
            self.require_folder(&actor.id, parent).await?;
        }
        self.repository
            .list_folders(&actor.id, parent.as_ref())
            .await
    }

    pub async fn create_folder(
        &self,
        actor: &Actor,
        name: String,
        parent: Option<FolderId>,
    ) -> Result<Folder, AssetError> {
        if let Some(parent) = parent.as_ref() {
            self.require_folder(&actor.id, parent).await?;
        }
        let name = sanitize_folder_name(&name)?;
        self.repository
            .create_folder(NewFolder {
                owner_id: actor.id.clone(),
                name,
                parent_id: parent,
            })
            .await
    }

    pub async fn delete_folder(&self, actor: &Actor, id: FolderId) -> Result<bool, AssetError> {
        let Some(manifest) = self.repository.folder_tree_manifest(&actor.id, &id).await? else {
            return Ok(false);
        };
        let mut quarantined = Vec::new();
        for asset in &manifest.assets {
            match self
                .storage
                .quarantine(&AssetLocation(asset.file_path.clone()))
                .await
            {
                Ok(Some(item)) => quarantined.push(item),
                Ok(None) => {}
                Err(error) => {
                    for item in quarantined.iter().rev() {
                        let _ = self.storage.restore(item).await;
                    }
                    return Err(error);
                }
            }
        }
        match self
            .repository
            .delete_folder_tree(&actor.id, &manifest)
            .await
        {
            Ok(deleted) => {
                if deleted {
                    for item in quarantined {
                        self.storage.purge(item).await?;
                    }
                } else {
                    for item in quarantined.iter().rev() {
                        self.storage.restore(item).await?;
                    }
                }
                Ok(deleted)
            }
            Err(error) => {
                for item in quarantined.iter().rev() {
                    let _ = self.storage.restore(item).await;
                }
                Err(error)
            }
        }
    }

    async fn require_folder(&self, owner: &ActorId, id: &FolderId) -> Result<(), AssetError> {
        self.repository
            .get_folder(owner, id)
            .await?
            .map(|_| ())
            .ok_or(AssetError::NotFound)
    }
}

fn validate_bytes(bytes: &[u8]) -> Result<(), AssetError> {
    if bytes.is_empty() {
        return Err(AssetError::Invalid("上传文件为空".into()));
    }
    if bytes.len() > MAX_UPLOAD_BYTES {
        return Err(AssetError::Invalid("单文件不能超过 50MB".into()));
    }
    Ok(())
}

pub fn sanitize_filename(name: &str) -> String {
    let cleaned = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        "upload.bin".into()
    } else {
        cleaned
    }
}

fn sanitize_folder_name(name: &str) -> Result<String, AssetError> {
    let cleaned = name
        .chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string();
    if cleaned.is_empty() {
        Err(AssetError::Invalid("文件夹名称不能为空".into()))
    } else {
        Ok(cleaned)
    }
}

pub fn infer_file_type(name: &str, mime_type: &str) -> String {
    let extension = Path::new(name)
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_lowercase();
    match extension.as_str() {
        "ppt" | "pptx" => "ppt".into(),
        "doc" | "docx" | "md" | "markdown" | "txt" | "pdf" => "doc".into(),
        "xls" | "xlsx" | "csv" | "tsv" => "excel".into(),
        "drawio" | "xml" => "drawio".into(),
        "png" | "jpg" | "jpeg" | "webp" | "gif" | "svg" => "image".into(),
        "mp4" | "webm" | "avi" | "mov" | "mkv" | "flv" | "wmv" | "m4v" | "3gp" | "ogv" => {
            "video".into()
        }
        _ if mime_type.starts_with("image/") => "image".into(),
        _ if mime_type.starts_with("video/") => "video".into(),
        _ if mime_type.contains("spreadsheet") || mime_type.contains("excel") => "excel".into(),
        _ if mime_type.contains("presentation") => "ppt".into(),
        _ if mime_type.contains("pdf")
            || mime_type.contains("word")
            || mime_type.starts_with("text/") =>
        {
            "doc".into()
        }
        _ => "other".into(),
    }
}
