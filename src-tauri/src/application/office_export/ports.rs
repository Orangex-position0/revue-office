use async_trait::async_trait;

use super::{DocumentExportRequest, ExportedFile, OfficeExportError, SpreadsheetExportRequest};

#[async_trait]
pub trait DocumentExporter: Send + Sync {
    async fn export(
        &self,
        request: DocumentExportRequest,
    ) -> Result<ExportedFile, OfficeExportError>;
}

#[async_trait]
pub trait SpreadsheetExporter: Send + Sync {
    async fn export(
        &self,
        request: SpreadsheetExportRequest,
    ) -> Result<ExportedFile, OfficeExportError>;
}

#[async_trait]
pub trait ExportFileStore: Send + Sync {
    async fn save(&self, file: &ExportedFile) -> Result<(), OfficeExportError>;
    async fn read(&self, filename: &str) -> Result<Option<ExportedFile>, OfficeExportError>;
}
