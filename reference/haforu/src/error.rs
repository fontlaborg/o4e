// this_file: src/error.rs

//! Error types for haforu.
//!
//! This module defines all error types used throughout the codebase,
//! with descriptive messages and context for debugging.

use std::path::PathBuf;
use thiserror::Error;

/// Main error type for haforu operations.
#[derive(Error, Debug)]
pub enum Error {
    /// Font file not found at specified path
    #[error("Font file not found: {path}")]
    FontNotFound { path: PathBuf },

    /// Invalid font format or corrupted font file
    #[error("Invalid font file at {path}: {reason}")]
    InvalidFont { path: PathBuf, reason: String },

    /// Unsupported font format
    #[error("Unsupported font format: {format} at {path}")]
    UnsupportedFormat { format: String, path: PathBuf },

    /// Font variation axis not found
    #[error("Unknown variation axis '{axis}' in font {path}. Available axes: {available:?}")]
    UnknownAxis {
        axis: String,
        path: PathBuf,
        available: Vec<String>,
    },

    /// Variation coordinate out of bounds
    #[error("Variation coordinate for axis '{axis}' out of bounds: {value} not in [{min}, {max}]")]
    CoordinateOutOfBounds {
        axis: String,
        value: f32,
        min: f32,
        max: f32,
    },

    /// Glyph not found in font
    #[error("Glyph ID {glyph_id} not found in font {path}")]
    GlyphNotFound { glyph_id: u32, path: PathBuf },

    /// Text shaping failed
    #[error("Failed to shape text '{text}' with font {path}: {reason}")]
    ShapingFailed {
        text: String,
        path: PathBuf,
        reason: String,
    },

    /// Rasterization failed
    #[error("Failed to rasterize glyph {glyph_id} from font {path}: {reason}")]
    RasterizationFailed {
        glyph_id: u32,
        path: PathBuf,
        reason: String,
    },

    /// Invalid job specification
    #[error("Invalid job specification: {reason}")]
    InvalidJobSpec { reason: String },

    /// Invalid rendering parameters
    #[error("Invalid rendering parameters: {reason}")]
    InvalidRenderParams { reason: String },

    /// I/O error
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    /// JSON parsing error
    #[error("JSON parse error: {0}")]
    JsonParse(#[from] serde_json::Error),

    /// Image encoding error
    #[error("Image encoding error: {0}")]
    ImageEncode(#[source] image::ImageError),

    /// Memory mapping error
    #[error("Failed to memory-map font file {path}: {source}")]
    Mmap {
        path: PathBuf,
        source: std::io::Error,
    },

    /// Internal error (should not happen in production)
    #[error("Internal error: {0}")]
    Internal(String),
}

/// Specialized Result type for haforu operations.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_display_font_not_found() {
        let err = Error::FontNotFound {
            path: PathBuf::from("/path/to/font.ttf"),
        };
        let msg = err.to_string();
        assert!(msg.contains("Font file not found"));
        assert!(msg.contains("/path/to/font.ttf"));
    }

    #[test]
    fn test_error_display_unknown_axis() {
        let err = Error::UnknownAxis {
            axis: "ZZZZ".to_string(),
            path: PathBuf::from("font.ttf"),
            available: vec!["wght".to_string(), "wdth".to_string()],
        };
        let msg = err.to_string();
        assert!(msg.contains("Unknown variation axis 'ZZZZ'"));
        assert!(msg.contains("wght"));
        assert!(msg.contains("wdth"));
    }
}
