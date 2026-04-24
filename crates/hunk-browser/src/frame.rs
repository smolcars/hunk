use std::sync::Arc;
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::session::BrowserFrameMetadata;

pub const BROWSER_FRAME_TARGET_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 60);

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

#[derive(Debug, Clone)]
pub struct BrowserFrameRateLimiter {
    min_interval: Duration,
    last_notification_at: Option<Instant>,
}

impl BrowserFrameRateLimiter {
    pub fn v1_60fps() -> Self {
        Self {
            min_interval: BROWSER_FRAME_TARGET_INTERVAL,
            last_notification_at: None,
        }
    }

    pub fn with_min_interval(min_interval: Duration) -> Self {
        Self {
            min_interval,
            last_notification_at: None,
        }
    }

    pub fn should_notify(&mut self, now: Instant) -> bool {
        if self
            .last_notification_at
            .is_none_or(|last| now.saturating_duration_since(last) >= self.min_interval)
        {
            self.last_notification_at = Some(now);
            return true;
        }

        false
    }

    pub fn min_interval(&self) -> Duration {
        self.min_interval
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
