use std::sync::Arc;

use crate::application::conversations::SessionRepository;
use crate::application::conversations::model::Conversation;
use crate::application::identity::Actor;
use crate::capabilities::presentation::{
    PresentationCapability, PresentationExport, PresentationProject, PresentationProjectUpdate,
};

use super::{
    NewProject, Project, ProjectError, ProjectId, ProjectListQuery, ProjectRepository,
    ProjectUpdate,
};

pub struct ProjectApplicationService {
    projects: Arc<dyn ProjectRepository>,
    conversations: Arc<dyn SessionRepository>,
    presentations: Arc<PresentationCapability>,
}

impl ProjectApplicationService {
    pub fn new(
        projects: Arc<dyn ProjectRepository>,
        conversations: Arc<dyn SessionRepository>,
        presentations: Arc<PresentationCapability>,
    ) -> Self {
        Self {
            projects,
            conversations,
            presentations,
        }
    }

    pub async fn create(
        &self,
        actor: &Actor,
        title: String,
        description: Option<String>,
        tool_kind: Option<String>,
    ) -> Result<Project, ProjectError> {
        let tool_kind = tool_kind.unwrap_or_else(|| "general".into());
        self.projects
            .create(NewProject {
                owner_id: actor.id.clone(),
                title,
                description,
                tool_kind,
            })
            .await
    }

    pub async fn list(
        &self,
        actor: &Actor,
        query: Option<String>,
    ) -> Result<Vec<Project>, ProjectError> {
        self.projects
            .list(ProjectListQuery {
                owner_id: actor.id.clone(),
                query,
            })
            .await
    }

    pub async fn get(&self, actor: &Actor, id: String) -> Result<Project, ProjectError> {
        self.projects
            .find(&actor.id, &ProjectId(id))
            .await?
            .ok_or(ProjectError::NotFound)
    }

    pub async fn update(
        &self,
        actor: &Actor,
        id: String,
        title: Option<String>,
        description: Option<Option<String>>,
        tool_kind: Option<String>,
    ) -> Result<Project, ProjectError> {
        self.projects
            .update(ProjectUpdate {
                owner_id: actor.id.clone(),
                id: ProjectId(id),
                title,
                description,
                tool_kind,
            })
            .await?
            .ok_or(ProjectError::NotFound)
    }

    pub async fn delete(&self, actor: &Actor, id: String) -> Result<bool, ProjectError> {
        self.projects.delete(&actor.id, &ProjectId(id)).await
    }

    pub async fn sessions(
        &self,
        actor: &Actor,
        id: String,
    ) -> Result<Vec<Conversation>, ProjectError> {
        let project = self.get(actor, id).await?;
        Ok(self
            .conversations
            .list_by_owner(&actor.id.0, 100, None)
            .await?
            .into_iter()
            .filter(|session| session.project_id.as_deref() == Some(project.id.as_str()))
            .collect())
    }

    pub async fn create_presentation(
        &self,
        actor: &Actor,
        title: String,
        theme: Option<String>,
    ) -> Result<PresentationProject, ProjectError> {
        Ok(self
            .presentations
            .create_project(
                &actor.id.0,
                title,
                theme.unwrap_or_else(|| "default".into()),
            )
            .await?)
    }

    pub async fn list_presentations(
        &self,
        actor: &Actor,
    ) -> Result<Vec<PresentationProject>, ProjectError> {
        Ok(self.presentations.list_projects(&actor.id.0).await?)
    }

    pub async fn get_presentation(
        &self,
        actor: &Actor,
        id: &str,
    ) -> Result<PresentationProject, ProjectError> {
        let project = self
            .presentations
            .get_project(id)
            .await?
            .ok_or(ProjectError::NotFound)?;
        if !actor.owns(&project.owner_id) {
            return Err(ProjectError::Forbidden);
        }
        Ok(project)
    }

    pub async fn update_presentation(
        &self,
        actor: &Actor,
        id: &str,
        title: Option<String>,
        theme: Option<String>,
    ) -> Result<PresentationProject, ProjectError> {
        let project = self.get_presentation(actor, id).await?;
        let title = title
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        let theme = theme
            .map(|value| value.trim().to_owned())
            .filter(|value| !value.is_empty());
        Ok(self
            .presentations
            .update_project(project, PresentationProjectUpdate { title, theme })
            .await?)
    }

    pub async fn delete_presentation(&self, actor: &Actor, id: &str) -> Result<bool, ProjectError> {
        self.get_presentation(actor, id).await?;
        Ok(self.presentations.delete_project(&actor.id.0, id).await?)
    }

    pub async fn export_presentation(
        &self,
        actor: &Actor,
        id: &str,
    ) -> Result<(PresentationProject, PresentationExport), ProjectError> {
        let project = self.get_presentation(actor, id).await?;
        if project.slides.is_empty() {
            return Err(ProjectError::Invalid("PPT 项目还没有幻灯片".into()));
        }
        let export = self.presentations.export_project(&project).await?;
        Ok((project, export))
    }
}
