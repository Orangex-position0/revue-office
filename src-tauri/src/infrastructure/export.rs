mod docx;
pub mod files;
mod pptx;
mod xlsx;

use async_trait::async_trait;

use crate::application::office_export::{
    DocumentExportRequest, DocumentExporter, ExportedFile, OfficeExportError,
    SpreadsheetExportRequest, SpreadsheetExporter,
};
use crate::capabilities::presentation::{
    PresentationExport, PresentationExportError, PresentationExporter, PresentationProject,
};

pub struct DocxDocumentExporter;
pub struct XlsxSpreadsheetExporter;
pub struct PptxPresentationExporter;

#[async_trait]
impl DocumentExporter for DocxDocumentExporter {
    async fn export(
        &self,
        request: DocumentExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        tokio::task::spawn_blocking(move || {
            let filename = format!("{}.docx", export_name(&request.title));
            let path = temporary_path("docx");
            let data = docx::DocData {
                title: request.title,
                sections: request
                    .sections
                    .into_iter()
                    .map(|section| docx::DocSection {
                        heading: section.heading,
                        heading_level: section.heading_level,
                        paragraphs: section.paragraphs,
                        bullets: section.bullets,
                        table: section.table.map(|table| docx::DocTable {
                            headers: table.headers,
                            rows: table.rows,
                        }),
                    })
                    .collect(),
            };
            render_file(&path, || docx::render_docx(&data, &path)).map(|bytes| ExportedFile {
                filename,
                content_type:
                    "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                        .into(),
                bytes,
            })
        })
        .await
        .map_err(|error| OfficeExportError::Render(anyhow::Error::new(error)))?
        .map_err(OfficeExportError::Render)
    }
}

#[async_trait]
impl SpreadsheetExporter for XlsxSpreadsheetExporter {
    async fn export(
        &self,
        request: SpreadsheetExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        tokio::task::spawn_blocking(move || {
            let filename = format!("{}.xlsx", export_name(&request.title));
            let path = temporary_path("xlsx");
            let data = xlsx::SheetData {
                title: request.title,
                tables: request
                    .tables
                    .into_iter()
                    .map(|table| xlsx::SheetTable {
                        title: table.title,
                        headers: table.headers,
                        rows: table.rows,
                    })
                    .collect(),
            };
            render_file(&path, || xlsx::render_xlsx(&data, &path)).map(|bytes| ExportedFile {
                filename,
                content_type: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
                    .into(),
                bytes,
            })
        })
        .await
        .map_err(|error| OfficeExportError::Render(anyhow::Error::new(error)))?
        .map_err(OfficeExportError::Render)
    }
}

#[async_trait]
impl PresentationExporter for PptxPresentationExporter {
    async fn export(
        &self,
        project: &PresentationProject,
    ) -> Result<PresentationExport, PresentationExportError> {
        let project = project.clone();
        tokio::task::spawn_blocking(move || {
            let path = temporary_path("pptx");
            render_file(&path, || pptx::render_pptx(&project, &path)).map(|bytes| {
                PresentationExport {
                    format: "pptx".into(),
                    bytes,
                }
            })
        })
        .await
        .map_err(|error| PresentationExportError::Failed(anyhow::Error::new(error)))?
        .map_err(PresentationExportError::Failed)
    }
}

fn temporary_path(extension: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "revue-office-export-{}.{}",
        uuid::Uuid::new_v4(),
        extension
    ))
}

fn render_file(
    path: &std::path::Path,
    render: impl FnOnce() -> anyhow::Result<()>,
) -> anyhow::Result<Vec<u8>> {
    let result = render().and_then(|()| std::fs::read(path).map_err(anyhow::Error::new));
    let cleanup = std::fs::remove_file(path);
    match result {
        Ok(bytes) => {
            if let Err(error) = cleanup
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(anyhow::Error::new(error).context("remove export temporary file"));
            }
            Ok(bytes)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

fn export_name(title: &str) -> String {
    let name = crate::application::office_export::sanitize_filename(title);
    if name.is_empty() {
        "output".into()
    } else {
        name
    }
}
