use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::application::identity::ActorId;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AssetId(pub String);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FolderId(pub String);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Asset {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub file_path: String,
    pub file_type: String,
    pub file_size: i64,
    pub folder_id: Option<String>,
    pub description: Option<String>,
    pub metadata: Option<Value>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct AssetStats {
    pub by_type: std::collections::HashMap<String, i64>,
    pub total_size: i64,
    pub total_files: i64,
}

#[derive(Clone, Debug)]
pub struct NewAsset {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
    pub folder_id: Option<FolderId>,
    pub description: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewAssetRecord {
    pub owner_id: ActorId,
    pub name: String,
    pub location: AssetLocation,
    pub file_type: String,
    pub file_size: i64,
    pub folder_id: Option<FolderId>,
    pub description: Option<String>,
    pub metadata: Option<Value>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssetLocation(pub String);

#[derive(Clone, Debug)]
pub struct AssetWrite {
    pub owner_id: ActorId,
    pub name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug)]
pub struct StoredAsset {
    pub location: AssetLocation,
}

#[derive(Clone, Debug)]
pub struct QuarantinedAsset {
    pub original: AssetLocation,
    pub quarantine: AssetLocation,
}

#[derive(Clone, Debug)]
pub struct AssetListQuery {
    pub owner_id: ActorId,
    pub folder_id: Option<FolderId>,
}

#[derive(Clone, Debug)]
pub struct AssetSearchQuery {
    pub owner_id: ActorId,
    pub query: Option<String>,
}

#[derive(Clone, Debug)]
pub struct NewFolder {
    pub owner_id: ActorId,
    pub name: String,
    pub parent_id: Option<FolderId>,
}

#[derive(Clone, Debug)]
pub struct FolderTreeManifest {
    pub folder_ids: Vec<FolderId>,
    pub assets: Vec<Asset>,
}

#[derive(Clone, Debug)]
pub struct AssetContentInput {
    pub name: String,
    pub mime_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExtractedText {
    pub text: String,
    pub parser: String,
    pub truncated: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuredPreview {
    pub preview_type: String,
    pub data: Value,
    pub parser: String,
    pub truncated: bool,
}

#[derive(Clone, Debug)]
pub struct AssetContent {
    pub asset: Asset,
    pub bytes: Vec<u8>,
}
