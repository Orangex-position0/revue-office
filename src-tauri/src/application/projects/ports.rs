use async_trait::async_trait;

use crate::application::identity::ActorId;

use super::{NewProject, Project, ProjectError, ProjectId, ProjectListQuery, ProjectUpdate};

#[async_trait]
pub trait ProjectRepository: Send + Sync {
    async fn create(&self, project: NewProject) -> Result<Project, ProjectError>;
    async fn find(&self, owner: &ActorId, id: &ProjectId) -> Result<Option<Project>, ProjectError>;
    async fn list(&self, query: ProjectListQuery) -> Result<Vec<Project>, ProjectError>;
    async fn update(&self, command: ProjectUpdate) -> Result<Option<Project>, ProjectError>;
    async fn delete(&self, owner: &ActorId, id: &ProjectId) -> Result<bool, ProjectError>;
}
