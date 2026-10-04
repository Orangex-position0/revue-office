#[cfg(target_os = "macos")]
use std::path::PathBuf;

#[cfg(target_os = "macos")]
use serde::Deserialize;

#[derive(Debug, thiserror::Error)]
pub enum OcrError {
    #[error("native OCR is unsupported on this platform")]
    Unsupported,
    #[error("native OCR failed: {0}")]
    Failed(#[source] anyhow::Error),
}

#[cfg(target_os = "macos")]
#[derive(Debug, Deserialize)]
struct OcrResult {
    text: String,
}

pub struct NativeImageOcr;

impl NativeImageOcr {
    pub async fn extract(&self, bytes: Vec<u8>, mime_type: String) -> Result<String, OcrError> {
        #[cfg(not(target_os = "macos"))]
        {
            let _ = (bytes, mime_type);
            Err(OcrError::Unsupported)
        }

        #[cfg(target_os = "macos")]
        {
            tokio::task::spawn_blocking(move || extract_blocking(&bytes, &mime_type))
                .await
                .map_err(|error| OcrError::Failed(anyhow::Error::new(error)))?
        }
    }
}

#[cfg(target_os = "macos")]
fn extract_blocking(bytes: &[u8], mime_type: &str) -> Result<String, OcrError> {
    let path = std::env::temp_dir().join(format!(
        "revue-office-ocr-{}.{}",
        uuid::Uuid::new_v4(),
        extension_for_mime(mime_type)
    ));
    std::fs::write(&path, bytes).map_err(|error| OcrError::Failed(anyhow::Error::new(error)))?;
    let result = run_swift(&path);
    let cleanup = std::fs::remove_file(&path);
    match result {
        Ok(text) => {
            if let Err(error) = cleanup
                && error.kind() != std::io::ErrorKind::NotFound
            {
                return Err(OcrError::Failed(anyhow::anyhow!(
                    "remove OCR temporary file: {error}"
                )));
            }
            Ok(text)
        }
        Err(error) => {
            let _ = cleanup;
            Err(error)
        }
    }
}

#[cfg(target_os = "macos")]
fn run_swift(path: &std::path::Path) -> Result<String, OcrError> {
    let output = std::process::Command::new("/usr/bin/swift")
        .arg(script_path())
        .arg(path)
        .output()
        .map_err(|error| OcrError::Failed(anyhow::Error::new(error)))?;
    if !output.status.success() {
        return Err(OcrError::Failed(anyhow::anyhow!(
            "swift OCR process exited unsuccessfully: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    let stdout = String::from_utf8(output.stdout)
        .map_err(|error| OcrError::Failed(anyhow::Error::new(error)))?;
    let parsed: OcrResult = serde_json::from_str(stdout.trim())
        .map_err(|error| OcrError::Failed(anyhow::Error::new(error)))?;
    Ok(parsed.text.trim().to_string())
}

#[cfg(target_os = "macos")]
fn extension_for_mime(mime_type: &str) -> &'static str {
    match mime_type {
        "image/png" => "png",
        "image/jpeg" | "image/jpg" => "jpg",
        "image/webp" => "webp",
        "image/gif" => "gif",
        "image/svg+xml" => "svg",
        _ => "png",
    }
}

#[cfg(target_os = "macos")]
fn script_path() -> PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("scripts")
        .join("image_ocr.swift")
}
