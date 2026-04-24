use std::sync::Arc;

use thiserror::Error;

use crate::session::BrowserFrameMetadata;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserFrame {
    metadata: BrowserFrameMetadata,
    bgra: Arc<[u8]>,
}

impl BrowserFrame {
    pub fn from_bgra(
        width: u32,
        height: u32,
        frame_epoch: u64,
        bgra: impl Into<Vec<u8>>,
    ) -> Result<Self, BrowserFrameError> {
        if width == 0 || height == 0 {
            return Err(BrowserFrameError::InvalidDimensions { width, height });
        }

        let bgra = bgra.into();
        let expected_len = bgra_len(width, height)?;
        if bgra.len() != expected_len {
            return Err(BrowserFrameError::InvalidBufferLength {
                expected: expected_len,
                actual: bgra.len(),
            });
        }

        Ok(Self {
            metadata: BrowserFrameMetadata {
                width,
                height,
                frame_epoch,
            },
            bgra: Arc::from(bgra),
        })
    }

    pub fn metadata(&self) -> &BrowserFrameMetadata {
        &self.metadata
    }

    pub fn bgra(&self) -> &[u8] {
        self.bgra.as_ref()
    }

    pub fn is_blank(&self) -> bool {
        self.bgra.iter().all(|channel| *channel == 0)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BrowserFrameError {
    #[error("browser frame dimensions must be non-zero, received {width}x{height}")]
    InvalidDimensions { width: u32, height: u32 },
    #[error("browser frame buffer length mismatch; expected {expected} bytes, received {actual}")]
    InvalidBufferLength { expected: usize, actual: usize },
    #[error("browser frame dimensions are too large")]
    DimensionsTooLarge,
}

fn bgra_len(width: u32, height: u32) -> Result<usize, BrowserFrameError> {
    (width as usize)
        .checked_mul(height as usize)
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(BrowserFrameError::DimensionsTooLarge)
}
