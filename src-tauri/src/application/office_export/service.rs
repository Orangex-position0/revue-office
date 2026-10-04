use std::sync::Arc;

use super::{
    DocumentExportRequest, DocumentExporter, ExportFileStore, ExportedFile, OfficeExportError,
    SpreadsheetExportRequest, SpreadsheetExporter,
};

pub struct OfficeExportApplicationService {
    documents: Arc<dyn DocumentExporter>,
    spreadsheets: Arc<dyn SpreadsheetExporter>,
    files: Arc<dyn ExportFileStore>,
}

impl OfficeExportApplicationService {
    pub fn new(
        documents: Arc<dyn DocumentExporter>,
        spreadsheets: Arc<dyn SpreadsheetExporter>,
        files: Arc<dyn ExportFileStore>,
    ) -> Self {
        Self {
            documents,
            spreadsheets,
            files,
        }
    }

    pub async fn export_document(
        &self,
        request: DocumentExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        validate_title(&request.title)?;
        let file = self.documents.export(request).await?;
        validate_exported_file(&file)?;
        self.files.save(&file).await?;
        Ok(file)
    }

    pub async fn export_spreadsheet(
        &self,
        request: SpreadsheetExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        validate_title(&request.title)?;
        let file = self.spreadsheets.export(request).await?;
        validate_exported_file(&file)?;
        self.files.save(&file).await?;
        Ok(file)
    }

    pub async fn download(&self, filename: &str) -> Result<ExportedFile, OfficeExportError> {
        let filename = sanitize_filename(filename);
        if filename.is_empty() {
            return Err(OfficeExportError::Invalid("filename is empty".into()));
        }
        self.files
            .read(&filename)
            .await?
            .ok_or(OfficeExportError::NotFound)
    }
}

fn validate_title(title: &str) -> Result<(), OfficeExportError> {
    if title.trim().is_empty() {
        Err(OfficeExportError::Invalid("title is empty".into()))
    } else {
        Ok(())
    }
}

fn validate_exported_file(file: &ExportedFile) -> Result<(), OfficeExportError> {
    if file.bytes.is_empty() {
        return Err(OfficeExportError::Render(anyhow::anyhow!(
            "renderer returned empty output"
        )));
    }
    if file.filename.is_empty() || file.content_type.is_empty() {
        return Err(OfficeExportError::Render(anyhow::anyhow!(
            "renderer returned incomplete metadata"
        )));
    }
    Ok(())
}

pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| !matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'))
        .collect::<String>()
        .trim()
        .to_string()
}
