// this_file: backends/o4e-core/src/error.rs

//! Error types for the o4e rendering engine.

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for o4e operations
#[derive(Error, Debug)]
pub enum O4eError {
    /// Font loading error
    #[error("Failed to load font: {path:?}")]
    FontLoadError {
        path: PathBuf,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Font not found
    #[error("Font not found: {name}")]
    FontNotFound { name: String },

    /// Invalid font data
    #[error("Invalid font data")]
    InvalidFontData,

    /// Shaping error
    #[error("Failed to shape text: {reason}")]
    ShapingError { reason: String },

    /// Rendering error
    #[error("Failed to render: {reason}")]
    RenderError { reason: String },

    /// Segmentation error
    #[error("Failed to segment text: {reason}")]
    SegmentationError { reason: String },

    /// Invalid parameter
    #[error("Invalid parameter: {name} = {value}")]
    InvalidParameter { name: String, value: String },

    /// Backend not available
    #[error("Backend not available: {name}")]
    BackendNotAvailable { name: String },

    /// IO error
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Generic error
    #[error("{0}")]
    Other(String),
}

impl O4eError {
    /// Create a new font load error
    pub fn font_load(
        path: PathBuf,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self::FontLoadError {
            path,
            source: Box::new(source),
        }
    }

    /// Create a new shaping error
    pub fn shaping(reason: impl Into<String>) -> Self {
        Self::ShapingError {
            reason: reason.into(),
        }
    }

    /// Create a new rendering error
    pub fn render(reason: impl Into<String>) -> Self {
        Self::RenderError {
            reason: reason.into(),
        }
    }

    /// Create a new segmentation error
    pub fn segmentation(reason: impl Into<String>) -> Self {
        Self::SegmentationError {
            reason: reason.into(),
        }
    }

    /// Create a new generic error
    pub fn other(msg: impl Into<String>) -> Self {
        Self::Other(msg.into())
    }
}
