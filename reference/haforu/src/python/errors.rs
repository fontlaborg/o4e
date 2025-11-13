// this_file: src/python/errors.rs

//! Error conversion from Rust to Python exceptions.
//!
//! This module provides centralized error handling for Python bindings,
//! converting haforu::Error variants to appropriate Python exception types
//! with enhanced context including job IDs, font paths, and detailed messages.

use crate::error::Error as HaforuError;
use pyo3::exceptions::{PyIOError, PyRuntimeError, PyValueError};
use pyo3::prelude::*;

/// Enhanced error converter with context support.
///
/// Provides methods to convert haforu errors to Python exceptions
/// with optional context like job IDs and additional information.
pub struct ErrorConverter;

impl ErrorConverter {
    /// Convert a haforu error to a Python exception with optional job context.
    ///
    /// # Arguments
    /// * `err` - The haforu error to convert
    /// * `job_id` - Optional job ID for context
    ///
    /// # Returns
    /// A PyErr that can be raised in Python
    pub fn to_pyerr(err: HaforuError, job_id: Option<&str>) -> PyErr {
        let context = job_id
            .map(|id| format!("[Job: {}] ", id))
            .unwrap_or_default();

        match err {
            // I/O Errors → PyIOError
            HaforuError::FontNotFound { path } => PyIOError::new_err(format!(
                "{}Font file not found: {}",
                context,
                path.display()
            )),

            HaforuError::Io(source) => {
                PyIOError::new_err(format!("{}I/O error: {}", context, source))
            }

            HaforuError::Mmap { path, source } => PyIOError::new_err(format!(
                "{}Failed to memory-map font file {}: {}",
                context,
                path.display(),
                source
            )),

            // Validation Errors → PyValueError
            HaforuError::InvalidJobSpec { reason } => {
                PyValueError::new_err(format!("{}Invalid job specification: {}", context, reason))
            }

            HaforuError::InvalidRenderParams { reason } => PyValueError::new_err(format!(
                "{}Invalid rendering parameters: {}",
                context, reason
            )),

            HaforuError::UnknownAxis {
                axis,
                path,
                available,
            } => PyValueError::new_err(format!(
                "{}Unknown variation axis '{}' in font {}. Available axes: {:?}",
                context,
                axis,
                path.display(),
                available
            )),

            HaforuError::CoordinateOutOfBounds {
                axis,
                value,
                min,
                max,
            } => PyValueError::new_err(format!(
                "{}Variation coordinate for axis '{}' out of bounds: {} not in [{}, {}]",
                context, axis, value, min, max
            )),

            HaforuError::JsonParse(source) => {
                PyValueError::new_err(format!("{}JSON parse error: {}", context, source))
            }

            // Runtime Errors → PyRuntimeError
            HaforuError::InvalidFont { path, reason } => PyRuntimeError::new_err(format!(
                "{}Invalid font file at {}: {}",
                context,
                path.display(),
                reason
            )),

            HaforuError::UnsupportedFormat { format, path } => PyRuntimeError::new_err(format!(
                "{}Unsupported font format '{}' at {}",
                context,
                format,
                path.display()
            )),

            HaforuError::GlyphNotFound { glyph_id, path } => PyRuntimeError::new_err(format!(
                "{}Glyph ID {} not found in font {}",
                context,
                glyph_id,
                path.display()
            )),

            HaforuError::ShapingFailed { text, path, reason } => PyRuntimeError::new_err(format!(
                "{}Failed to shape text '{}' with font {}: {}",
                context,
                text,
                path.display(),
                reason
            )),

            HaforuError::RasterizationFailed {
                glyph_id,
                path,
                reason,
            } => PyRuntimeError::new_err(format!(
                "{}Failed to rasterize glyph {} from font {}: {}",
                context,
                glyph_id,
                path.display(),
                reason
            )),

            HaforuError::ImageEncode(err) => {
                PyRuntimeError::new_err(format!("{}Image encoding error: {}", context, err))
            }

            HaforuError::Internal(msg) => {
                PyRuntimeError::new_err(format!("{}Internal error: {}", context, msg))
            }
        }
    }

    /// Convert a haforu error to PyErr without job context.
    ///
    /// This is a convenience method for cases where job ID is not available.
    pub fn to_pyerr_simple(err: HaforuError) -> PyErr {
        Self::to_pyerr(err, None)
    }
}

/// Direct conversion from HaforuError to PyErr for ergonomic use.
///
/// This implements the standard From trait for convenient ? operator usage.
/// For cases where you need job context, use ErrorConverter::to_pyerr directly.
impl From<HaforuError> for PyErr {
    fn from(err: HaforuError) -> PyErr {
        ErrorConverter::to_pyerr_simple(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_error_conversion_without_context() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // Test I/O error → PyIOError
            let err = HaforuError::FontNotFound {
                path: PathBuf::from("/nonexistent/font.ttf"),
            };
            let py_err = ErrorConverter::to_pyerr_simple(err);
            assert!(py_err.is_instance_of::<PyIOError>(py));
            let msg = py_err.to_string();
            assert!(msg.contains("Font file not found"));
            assert!(msg.contains("/nonexistent/font.ttf"));

            // Test validation error → PyValueError
            let err = HaforuError::InvalidRenderParams {
                reason: "Width must be positive".to_string(),
            };
            let py_err = ErrorConverter::to_pyerr_simple(err);
            assert!(py_err.is_instance_of::<PyValueError>(py));
            let msg = py_err.to_string();
            assert!(msg.contains("Invalid rendering parameters"));
            assert!(msg.contains("Width must be positive"));

            // Test runtime error → PyRuntimeError
            let err = HaforuError::ShapingFailed {
                text: "test".to_string(),
                path: PathBuf::from("font.ttf"),
                reason: "no glyphs found".to_string(),
            };
            let py_err = ErrorConverter::to_pyerr_simple(err);
            assert!(py_err.is_instance_of::<PyRuntimeError>(py));
            let msg = py_err.to_string();
            assert!(msg.contains("Failed to shape text"));
            assert!(msg.contains("test"));
        });
    }

    #[test]
    fn test_error_conversion_with_job_context() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // Test with job ID context
            let err = HaforuError::FontNotFound {
                path: PathBuf::from("/missing/font.ttf"),
            };
            let py_err = ErrorConverter::to_pyerr(err, Some("job_123"));
            let msg = py_err.to_string();
            assert!(msg.contains("[Job: job_123]"));
            assert!(msg.contains("Font file not found"));
            assert!(msg.contains("/missing/font.ttf"));

            // Test validation error with job context
            let err = HaforuError::InvalidJobSpec {
                reason: "Missing required field 'text'".to_string(),
            };
            let py_err = ErrorConverter::to_pyerr(err, Some("batch_42"));
            let msg = py_err.to_string();
            assert!(msg.contains("[Job: batch_42]"));
            assert!(msg.contains("Invalid job specification"));
            assert!(msg.contains("Missing required field 'text'"));

            // Test runtime error with job context
            let err = HaforuError::RasterizationFailed {
                glyph_id: 123,
                path: PathBuf::from("font.ttf"),
                reason: "out of memory".to_string(),
            };
            let py_err = ErrorConverter::to_pyerr(err, Some("render_999"));
            let msg = py_err.to_string();
            assert!(msg.contains("[Job: render_999]"));
            assert!(msg.contains("Failed to rasterize glyph 123"));
            assert!(msg.contains("out of memory"));
        });
    }

    #[test]
    fn test_from_trait_implementation() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // Test that From trait works for ? operator usage
            let err = HaforuError::InvalidRenderParams {
                reason: "test".to_string(),
            };
            let py_err: PyErr = err.into();
            assert!(py_err.is_instance_of::<PyValueError>(py));
        });
    }

    #[test]
    fn test_all_error_variants_mapped() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            // Ensure all error variants convert to appropriate exception types

            // FontNotFound → PyIOError
            let err = HaforuError::FontNotFound {
                path: PathBuf::from("test.ttf"),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyIOError>(py));

            // InvalidFont → PyRuntimeError
            let err = HaforuError::InvalidFont {
                path: PathBuf::from("test.ttf"),
                reason: "corrupted".to_string(),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));

            // UnsupportedFormat → PyRuntimeError
            let err = HaforuError::UnsupportedFormat {
                format: "woff2".to_string(),
                path: PathBuf::from("test.woff2"),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));

            // UnknownAxis → PyValueError
            let err = HaforuError::UnknownAxis {
                axis: "ZZZZ".to_string(),
                path: PathBuf::from("test.ttf"),
                available: vec!["wght".to_string()],
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyValueError>(py));

            // CoordinateOutOfBounds → PyValueError
            let err = HaforuError::CoordinateOutOfBounds {
                axis: "wght".to_string(),
                value: 1000.0,
                min: 100.0,
                max: 900.0,
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyValueError>(py));

            // GlyphNotFound → PyRuntimeError
            let err = HaforuError::GlyphNotFound {
                glyph_id: 999,
                path: PathBuf::from("test.ttf"),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));

            // ShapingFailed → PyRuntimeError
            let err = HaforuError::ShapingFailed {
                text: "test".to_string(),
                path: PathBuf::from("test.ttf"),
                reason: "failed".to_string(),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));

            // RasterizationFailed → PyRuntimeError
            let err = HaforuError::RasterizationFailed {
                glyph_id: 1,
                path: PathBuf::from("test.ttf"),
                reason: "failed".to_string(),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));

            // InvalidJobSpec → PyValueError
            let err = HaforuError::InvalidJobSpec {
                reason: "bad".to_string(),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyValueError>(py));

            // InvalidRenderParams → PyValueError
            let err = HaforuError::InvalidRenderParams {
                reason: "bad".to_string(),
            };
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyValueError>(py));

            // Internal → PyRuntimeError
            let err = HaforuError::Internal("bug".to_string());
            assert!(ErrorConverter::to_pyerr_simple(err).is_instance_of::<PyRuntimeError>(py));
        });
    }

    #[test]
    fn test_error_messages_include_all_context() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|_py| {
            // Test UnknownAxis includes all available axes
            let err = HaforuError::UnknownAxis {
                axis: "ZZZZ".to_string(),
                path: PathBuf::from("font.ttf"),
                available: vec!["wght".to_string(), "wdth".to_string(), "slnt".to_string()],
            };
            let py_err = ErrorConverter::to_pyerr(err, Some("test_job"));
            let msg = py_err.to_string();
            assert!(msg.contains("[Job: test_job]"));
            assert!(msg.contains("ZZZZ"));
            assert!(msg.contains("wght"));
            assert!(msg.contains("wdth"));
            assert!(msg.contains("slnt"));

            // Test CoordinateOutOfBounds includes all bounds
            let err = HaforuError::CoordinateOutOfBounds {
                axis: "wght".to_string(),
                value: 1000.0,
                min: 100.0,
                max: 900.0,
            };
            let py_err = ErrorConverter::to_pyerr(err, Some("test_job"));
            let msg = py_err.to_string();
            assert!(msg.contains("wght"));
            assert!(msg.contains("1000"));
            assert!(msg.contains("100"));
            assert!(msg.contains("900"));
        });
    }
}
