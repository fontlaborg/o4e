// this_file: src/security.rs

//! Security and validation utilities for haforu.
//!
//! Provides basic path sanitization, input size limits, text validation,
//! font size checks, and simple timeouts. Kept minimal to avoid bloat while
//! aligning with safeguards present in the legacy `haforu` implementation.

use crate::error::{Error, Result};
use camino::{Utf8Path, Utf8PathBuf};
use std::time::{Duration, Instant};

/// Maximum allowed JSON input size (10MB)
pub const MAX_JSON_SIZE: usize = 10 * 1024 * 1024;
/// Maximum allowed number of jobs per spec
pub const MAX_JOBS_PER_SPEC: usize = 1000;
/// Maximum allowed text length
pub const MAX_TEXT_LENGTH: usize = 10_000;
/// Maximum allowed font file size (50MB)
pub const MAX_FONT_SIZE: u64 = 50 * 1024 * 1024;
/// Default per-job timeout
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Validate and sanitize a font path against an optional base directory.
/// Returns a canonical absolute path if valid.
pub fn sanitize_path(path: &Utf8Path, base_dir: Option<&Utf8Path>) -> Result<Utf8PathBuf> {
    let path_str = path.as_str();
    if path_str.contains("..") || path_str.contains('~') {
        return Err(Error::InvalidJobSpec {
            reason: "Path contains invalid components (.. or ~)".to_string(),
        });
    }

    // Resolve to absolute path using either provided base_dir or CWD
    let abs = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(base) = base_dir {
        base.join(path)
    } else {
        let cwd = std::env::current_dir()
            .map_err(|e| Error::Internal(format!("Failed to get current dir: {}", e)))?;
        let cwd = Utf8PathBuf::from_path_buf(cwd)
            .map_err(|_| Error::Internal("Non-UTF8 current working directory".to_string()))?;
        cwd.join(path)
    };

    // Canonicalize and ensure it remains within base_dir if provided
    let canonical_std =
        std::fs::canonicalize(abs.as_std_path()).map_err(|e| Error::InvalidJobSpec {
            reason: format!("Cannot resolve path {}: {}", abs, e),
        })?;

    let canonical = Utf8PathBuf::from_path_buf(canonical_std)
        .map_err(|_| Error::Internal("Canonical path is not valid UTF-8".to_string()))?;

    if let Some(base) = base_dir {
        let base_canon_std =
            std::fs::canonicalize(base.as_std_path()).map_err(|e| Error::InvalidJobSpec {
                reason: format!("Cannot resolve base path {}: {}", base, e),
            })?;
        let base_canon = Utf8PathBuf::from_path_buf(base_canon_std)
            .map_err(|_| Error::Internal("Canonical base path is not valid UTF-8".to_string()))?;
        if !canonical.as_str().starts_with(base_canon.as_str()) {
            return Err(Error::InvalidJobSpec {
                reason: format!(
                    "Path {} is outside allowed base directory {}",
                    canonical, base_canon
                ),
            });
        }
    }

    Ok(canonical)
}

/// Validate JSON input size
pub fn validate_json_size(json: &str, max_bytes: usize) -> Result<()> {
    if json.len() > max_bytes {
        return Err(Error::InvalidJobSpec {
            reason: format!(
                "JSON input too large: {} bytes (max: {} bytes)",
                json.len(),
                max_bytes
            ),
        });
    }
    Ok(())
}

/// Validate text content for shaping.
pub fn validate_text_input(text: &str) -> Result<()> {
    if text.len() > MAX_TEXT_LENGTH {
        return Err(Error::InvalidJobSpec {
            reason: format!(
                "Text too long: {} characters (max: {} characters)",
                text.len(),
                MAX_TEXT_LENGTH
            ),
        });
    }
    if text.chars().any(|c| c.is_control() && !c.is_whitespace()) {
        return Err(Error::InvalidJobSpec {
            reason: "Text contains invalid control characters".to_string(),
        });
    }
    Ok(())
}

/// Validate font file size before mapping.
pub fn validate_font_size(size_bytes: u64) -> Result<()> {
    if size_bytes > MAX_FONT_SIZE {
        return Err(Error::InvalidJobSpec {
            reason: format!(
                "Font file too large: {} bytes (max: {} bytes)",
                size_bytes, MAX_FONT_SIZE
            ),
        });
    }
    Ok(())
}

/// Simple timeout guard for per-job operations.
pub struct TimeoutGuard {
    start: Instant,
    timeout: Duration,
}

impl TimeoutGuard {
    pub fn new(timeout: Duration) -> Self {
        Self {
            start: Instant::now(),
            timeout,
        }
    }

    pub fn check(&self, label: &str) -> Result<()> {
        if self.start.elapsed() > self.timeout {
            return Err(Error::InvalidRenderParams {
                reason: format!("Operation '{}' timed out after {:?}", label, self.timeout),
            });
        }
        Ok(())
    }
}
