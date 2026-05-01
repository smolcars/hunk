use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MobileFrameMetadata {
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub frame_epoch: u64,
    pub byte_len: usize,
    pub media_type: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MobileFrame {
    metadata: MobileFrameMetadata,
    png: Vec<u8>,
}

impl MobileFrame {
    pub fn from_png(
        png: Vec<u8>,
        frame_epoch: u64,
        dimensions: Option<(u32, u32)>,
    ) -> Result<Self, MobileFrameError> {
        if png.is_empty() {
            return Err(MobileFrameError::EmptyPng);
        }
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err(MobileFrameError::InvalidPngSignature);
        }
        let (width, height) = dimensions
            .map(|(width, height)| (Some(width), Some(height)))
            .unwrap_or((None, None));
        Ok(Self {
            metadata: MobileFrameMetadata {
                width,
                height,
                frame_epoch,
                byte_len: png.len(),
                media_type: "image/png".to_string(),
            },
            png,
        })
    }

    pub fn metadata(&self) -> &MobileFrameMetadata {
        &self.metadata
    }

    pub fn png(&self) -> &[u8] {
        self.png.as_slice()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum MobileFrameError {
    #[error("mobile screenshot PNG is empty")]
    EmptyPng,
    #[error("mobile screenshot does not have a PNG signature")]
    InvalidPngSignature,
}
