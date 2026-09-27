use async_trait::async_trait;
use thiserror::Error;

use crate::contracts::artifact::{
    ArtifactFinalization, ArtifactPublication, NewArtifactPublication,
};

#[derive(Debug, Error)]
pub enum ArtifactPublicationRepositoryError {
    #[error("artifact publication repository is unavailable")]
    Unavailable(#[source] anyhow::Error),
    #[error("artifact publication data is invalid: {0}")]
    InvalidData(String),
    #[error("artifact publication state conflict")]
    Conflict,
}

#[async_trait]
pub trait ArtifactPublicationRepository: Send + Sync {
    async fn reserve(
        &self,
        publication: NewArtifactPublication,
    ) -> Result<ArtifactPublication, ArtifactPublicationRepositoryError>;

    async fn find(
        &self,
        id: &str,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError>;

    async fn finalize(
        &self,
        id: &str,
        finalization: ArtifactFinalization,
    ) -> Result<Option<ArtifactPublication>, ArtifactPublicationRepositoryError>;

    async fn fail(&self, id: &str, error: &str)
        -> Result<bool, ArtifactPublicationRepositoryError>;

    async fn pending(&self)
        -> Result<Vec<ArtifactPublication>, ArtifactPublicationRepositoryError>;
}
