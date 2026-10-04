mod office;

use async_trait::async_trait;
use serde_json::json;

use crate::application::assets::{
    AssetContentExtractor, AssetContentInput, AssetError, ExtractedText, StructuredPreview,
};
use crate::infrastructure::ocr::{NativeImageOcr, OcrError};

pub struct OfficeAssetContentExtractor {
    ocr: NativeImageOcr,
}

impl OfficeAssetContentExtractor {
    pub fn new() -> Self {
        Self {
            ocr: NativeImageOcr,
        }
    }

    async fn extract_image(&self, input: AssetContentInput) -> Result<ExtractedText, AssetError> {
        let text = self
            .ocr
            .extract(input.bytes, input.mime_type)
            .await
            .map_err(|error| match error {
                OcrError::Unsupported => AssetError::Unsupported("native OCR".into()),
                OcrError::Failed(error) => AssetError::Extraction(error),
            })?;
        Ok(ExtractedText {
            text,
            parser: "native_ocr".into(),
            truncated: false,
        })
    }
}

impl Default for OfficeAssetContentExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AssetContentExtractor for OfficeAssetContentExtractor {
    async fn extract_text(&self, input: AssetContentInput) -> Result<ExtractedText, AssetError> {
        if input.mime_type.starts_with("image/") {
            return self.extract_image(input).await;
        }
        tokio::task::spawn_blocking(move || {
            let value =
                office::extract_text_from_bytes(&input.name, &input.mime_type, &input.bytes);
            if value.parser == "error" {
                return Err(AssetError::Extraction(anyhow::anyhow!(value.text)));
            }
            Ok(ExtractedText {
                text: value.text,
                parser: value.parser,
                truncated: value.truncated,
            })
        })
        .await
        .map_err(|error| AssetError::Extraction(anyhow::Error::new(error)))?
    }

    async fn extract_structured(
        &self,
        input: AssetContentInput,
    ) -> Result<StructuredPreview, AssetError> {
        if input.mime_type.starts_with("image/") {
            let extracted = self.extract_image(input).await?;
            return Ok(StructuredPreview {
                preview_type: "text".into(),
                data: json!({ "text": extracted.text }),
                parser: extracted.parser,
                truncated: extracted.truncated,
            });
        }
        tokio::task::spawn_blocking(move || {
            let value = office::extract_structured(&input.name, &input.mime_type, &input.bytes);
            if value.parser == "error" {
                return Err(AssetError::Extraction(anyhow::anyhow!(
                    "structured extraction failed"
                )));
            }
            Ok(StructuredPreview {
                preview_type: value.preview_type,
                data: value.data,
                parser: value.parser,
                truncated: value.truncated,
            })
        })
        .await
        .map_err(|error| AssetError::Extraction(anyhow::Error::new(error)))?
    }
}
