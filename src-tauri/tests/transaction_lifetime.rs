use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use revue_office_lib::application::office_export::{
    DocumentExportRequest, DocumentExporter, ExportFileStore, ExportedFile,
    OfficeExportApplicationService, OfficeExportError, SpreadsheetExportRequest,
    SpreadsheetExporter,
};

struct ObservingDocumentExporter {
    transaction_active: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl DocumentExporter for ObservingDocumentExporter {
    async fn export(
        &self,
        _request: DocumentExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        assert!(
            !self.transaction_active.load(Ordering::SeqCst),
            "long renderer I/O started while a repository transaction was active"
        );
        self.events.lock().unwrap().push("render");
        Ok(ExportedFile {
            filename: "output.docx".into(),
            content_type: "application/docx".into(),
            bytes: vec![1],
        })
    }
}

struct UnusedSpreadsheetExporter;

#[async_trait]
impl SpreadsheetExporter for UnusedSpreadsheetExporter {
    async fn export(
        &self,
        _request: SpreadsheetExportRequest,
    ) -> Result<ExportedFile, OfficeExportError> {
        unreachable!("spreadsheet exporter is not part of this scenario")
    }
}

struct ObservingFileStore {
    transaction_active: Arc<AtomicBool>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl ExportFileStore for ObservingFileStore {
    async fn save(&self, _file: &ExportedFile) -> Result<(), OfficeExportError> {
        assert!(
            !self.transaction_active.load(Ordering::SeqCst),
            "file I/O started while a repository transaction was active"
        );
        self.events.lock().unwrap().push("save");
        Ok(())
    }

    async fn read(&self, _filename: &str) -> Result<Option<ExportedFile>, OfficeExportError> {
        unreachable!("download is not part of this scenario")
    }
}

#[tokio::test]
async fn direct_export_performs_long_io_without_repository_transaction_scope() {
    let transaction_active = Arc::new(AtomicBool::new(false));
    let events = Arc::new(Mutex::new(Vec::new()));
    let service = OfficeExportApplicationService::new(
        Arc::new(ObservingDocumentExporter {
            transaction_active: transaction_active.clone(),
            events: events.clone(),
        }),
        Arc::new(UnusedSpreadsheetExporter),
        Arc::new(ObservingFileStore {
            transaction_active,
            events: events.clone(),
        }),
    );
    service
        .export_document(DocumentExportRequest {
            title: "Output".into(),
            sections: vec![],
        })
        .await
        .unwrap();
    assert_eq!(*events.lock().unwrap(), vec!["render", "save"]);
}

#[test]
fn sql_adapters_do_not_call_filesystem_renderer_extractor_or_ocr_inside_transactions() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let persistence = [
        root.join("infrastructure/persistence/sqlite/assets.rs"),
        root.join("infrastructure/persistence/mysql/assets.rs"),
        root.join("infrastructure/persistence/sqlite/projects.rs"),
        root.join("infrastructure/persistence/mysql/projects.rs"),
    ]
    .into_iter()
    .map(|path| std::fs::read_to_string(path).unwrap())
    .collect::<Vec<_>>()
    .join("\n");
    for forbidden in [
        "tokio::fs",
        "std::fs",
        "crate::infrastructure::export",
        "crate::infrastructure::extraction",
        "crate::infrastructure::ocr",
        "spawn_blocking",
        "Command::new",
    ] {
        assert!(
            !persistence.contains(forbidden),
            "persistence adapter must not perform long I/O via {forbidden}"
        );
    }

    let extraction = std::fs::read_to_string(root.join("infrastructure/extraction.rs")).unwrap();
    assert!(extraction.contains("spawn_blocking"));
    let export = std::fs::read_to_string(root.join("infrastructure/export.rs")).unwrap();
    assert!(export.contains("spawn_blocking"));
    let ocr = std::fs::read_to_string(root.join("infrastructure/ocr/native_swift.rs")).unwrap();
    assert!(ocr.contains("remove_file"));
}
