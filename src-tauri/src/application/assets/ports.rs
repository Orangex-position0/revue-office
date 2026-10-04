use async_trait::async_trait;

use crate::application::identity::ActorId;

use super::error::AssetError;
use super::model::*;

#[async_trait]
pub trait AssetRepository: Send + Sync {
    async fn create(&self, asset: NewAssetRecord) -> Result<Asset, AssetError>;
    async fn get(&self, owner: &ActorId, id: &AssetId) -> Result<Option<Asset>, AssetError>;
    async fn list(&self, query: AssetListQuery) -> Result<Vec<Asset>, AssetError>;
    async fn search(&self, query: AssetSearchQuery) -> Result<Vec<Asset>, AssetError>;
    async fn stats(&self, owner: &ActorId) -> Result<AssetStats, AssetError>;
    async fn delete(&self, owner: &ActorId, id: &AssetId) -> Result<bool, AssetError>;

    async fn get_folder(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<Folder>, AssetError>;
    async fn list_folders(
        &self,
        owner: &ActorId,
        parent: Option<&FolderId>,
    ) -> Result<Vec<Folder>, AssetError>;
    async fn create_folder(&self, folder: NewFolder) -> Result<Folder, AssetError>;
    async fn folder_tree_manifest(
        &self,
        owner: &ActorId,
        id: &FolderId,
    ) -> Result<Option<FolderTreeManifest>, AssetError>;
    async fn delete_folder_tree(
        &self,
        owner: &ActorId,
        manifest: &FolderTreeManifest,
    ) -> Result<bool, AssetError>;
}

#[async_trait]
pub trait AssetStorage: Send + Sync {
    async fn write(&self, request: AssetWrite) -> Result<StoredAsset, AssetError>;
    async fn read(&self, location: &AssetLocation) -> Result<Vec<u8>, AssetError>;
    async fn quarantine(
        &self,
        location: &AssetLocation,
    ) -> Result<Option<QuarantinedAsset>, AssetError>;
    async fn restore(&self, asset: &QuarantinedAsset) -> Result<(), AssetError>;
    async fn purge(&self, asset: QuarantinedAsset) -> Result<(), AssetError>;
}

#[async_trait]
pub trait AssetContentExtractor: Send + Sync {
    async fn extract_text(&self, input: AssetContentInput) -> Result<ExtractedText, AssetError>;
    async fn extract_structured(
        &self,
        input: AssetContentInput,
    ) -> Result<StructuredPreview, AssetError>;
}
