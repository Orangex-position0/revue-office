use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::assets::{AssetContentExtractor, AssetContentInput, AssetError};
use revue_office_lib::application::office_export::{
    DocumentExportRequest, DocumentExporter, DocumentSection, ExportFileStore, ExportedFile,
    OfficeExportApplicationService, OfficeExportError, SpreadsheetExportRequest,
    SpreadsheetExporter, SpreadsheetTable,
};
use revue_office_lib::infrastructure::export::files::LocalExportFileStore;
use revue_office_lib::infrastructure::export::{DocxDocumentExporter, XlsxSpreadsheetExporter};
use revue_office_lib::infrastructure::extraction::OfficeAssetContentExtractor;

struct FakeDocumentExporter {
    fail: bool,
}

#[async_trait]
impl DocumentExporter for FakeDocumentExporter {
    async fn export(
        &self,
        request: DocumentExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        if self.fail {
            return Err(OfficeExportError::Render(anyhow::anyhow!("render failed")));
        }
        Ok(ExportedFile {
            filename: format!("{}.docx", request.title),
            content_type: "application/docx".into(),
            bytes: b"docx".to_vec(),
        })
    }
}

struct FakeSpreadsheetExporter;

#[async_trait]
impl SpreadsheetExporter for FakeSpreadsheetExporter {
    async fn export(
        &self,
        request: SpreadsheetExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        Ok(ExportedFile {
            filename: format!("{}.xlsx", request.title),
            content_type: "application/xlsx".into(),
            bytes: b"xlsx".to_vec(),
        })
    }
}

#[derive(Default)]
struct MemoryFiles {
    values: Mutex<HashMap<String, ExportedFile>>,
    fail_save: bool,
}

#[async_trait]
impl ExportFileStore for MemoryFiles {
    async fn save(&self, file: &ExportedFile) -> Result<(), OfficeExportError> {
        if self.fail_save {
            return Err(OfficeExportError::Storage(anyhow::anyhow!("save failed")));
        }
        self.values
            .lock()
            .unwrap()
            .insert(file.filename.clone(), file.clone());
        Ok(())
    }

    async fn read(&self, filename: &str) -> Result<Option<ExportedFile>, OfficeExportError> {
        Ok(self.values.lock().unwrap().get(filename).cloned())
    }
}

fn document_request() -> DocumentExportRequest {
    DocumentExportRequest {
        title: "Delivery".into(),
        sections: vec![DocumentSection {
            heading: "Summary".into(),
            heading_level: 1,
            paragraphs: vec!["Ready".into()],
            bullets: vec![],
            table: None,
        }],
    }
}

#[tokio::test]
async fn successful_export_is_persisted_and_downloadable() {
    let files = Arc::new(MemoryFiles::default());
    let service = OfficeExportApplicationService::new(
        Arc::new(FakeDocumentExporter { fail: false }),
        Arc::new(FakeSpreadsheetExporter),
        files.clone(),
    );
    let file = service.export_document(document_request()).await.unwrap();
    assert_eq!(file.bytes, b"docx");
    assert_eq!(service.download("Delivery.docx").await.unwrap(), file);
    assert!(files.values.lock().unwrap().contains_key("Delivery.docx"));
}

#[tokio::test]
async fn renderer_or_file_store_failure_never_reports_export_success() {
    let files = Arc::new(MemoryFiles::default());
    let render_failure = OfficeExportApplicationService::new(
        Arc::new(FakeDocumentExporter { fail: true }),
        Arc::new(FakeSpreadsheetExporter),
        files.clone(),
    );
    assert!(
        render_failure
            .export_document(document_request())
            .await
            .is_err()
    );
    assert!(files.values.lock().unwrap().is_empty());

    let save_failure = OfficeExportApplicationService::new(
        Arc::new(FakeDocumentExporter { fail: false }),
        Arc::new(FakeSpreadsheetExporter),
        Arc::new(MemoryFiles {
            fail_save: true,
            ..Default::default()
        }),
    );
    assert!(
        save_failure
            .export_document(document_request())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn concrete_docx_xlsx_and_file_adapters_preserve_download_behavior() {
    let root = std::env::temp_dir().join(format!("revue-office-export-{}", uuid::Uuid::new_v4()));
    let service = OfficeExportApplicationService::new(
        Arc::new(DocxDocumentExporter),
        Arc::new(XlsxSpreadsheetExporter),
        Arc::new(LocalExportFileStore::new(&root)),
    );
    let docx = service.export_document(document_request()).await.unwrap();
    assert!(!docx.bytes.is_empty());
    assert_eq!(
        service.download(&docx.filename).await.unwrap().bytes,
        docx.bytes
    );
    let replacement = service.export_document(document_request()).await.unwrap();
    assert_eq!(
        service.download(&replacement.filename).await.unwrap().bytes,
        replacement.bytes
    );

    let xlsx = service
        .export_spreadsheet(SpreadsheetExportRequest {
            title: "Numbers".into(),
            tables: vec![SpreadsheetTable {
                title: "Sheet".into(),
                headers: vec!["Value".into()],
                rows: vec![vec!["42".into()]],
            }],
        })
        .await
        .unwrap();
    assert!(!xlsx.bytes.is_empty());
    assert_eq!(
        service.download(&xlsx.filename).await.unwrap().bytes,
        xlsx.bytes
    );
    let _ = tokio::fs::remove_dir_all(root).await;
}

#[tokio::test]
async fn extraction_failures_are_typed_and_ocr_does_not_fabricate_text() {
    let extractor = OfficeAssetContentExtractor::new();
    let malformed = extractor
        .extract_text(AssetContentInput {
            name: "broken.docx".into(),
            mime_type: "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
                .into(),
            bytes: b"not-a-zip".to_vec(),
        })
        .await;
    assert!(matches!(malformed, Err(AssetError::Extraction(_))));

    #[cfg(not(target_os = "macos"))]
    assert!(matches!(
        extractor
            .extract_text(AssetContentInput {
                name: "image.png".into(),
                mime_type: "image/png".into(),
                bytes: vec![1, 2, 3],
            })
            .await,
        Err(AssetError::Unsupported(_))
    ));
}

#[test]
fn direct_export_routes_and_download_headers_remain_compatible() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let handler =
        std::fs::read_to_string(root.join("transport/http/handlers/office_export.rs")).unwrap();
    for route in [
        "/api/doc/export",
        "/api/excel/export",
        "/api/files/download/:filename",
    ] {
        assert!(handler.contains(route), "missing compatible route {route}");
    }
    assert!(handler.contains("CONTENT_DISPOSITION"));
    assert!(!handler.contains("unwrap_or_default"));
    assert!(!handler.contains(".unwrap()"));
}
