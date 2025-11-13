// this_file: src/lib.rs

//! Haforu: High-performance batch font renderer for FontSimi.
//!
//! This library provides zero-copy font loading, text shaping, and rasterization
//! with support for variable fonts and batch processing via JSONL.
//!
//! ## Architecture
//!
//! - **batch**: Job specification and JSONL I/O
//! - **fonts**: Memory-mapped font loading and caching
//! - **shaping**: Text shaping with HarfBuzz
//! - **render**: Glyph rasterization with zeno
//! - **output**: PGM/PNG image generation
//! - **error**: Error types and handling
//!
//! ## Example
//!
//! ```rust,no_run
//! use haforu::{FontLoader, TextShaper, GlyphRasterizer, ImageOutput};
//! use std::collections::HashMap;
//! use camino::Utf8Path;
//!
//! // Load font with variations
//! let loader = FontLoader::new(512);
//! let mut coords = HashMap::new();
//! coords.insert("wght".to_string(), 600.0);
//! let font = loader.load_font(Utf8Path::new("font.ttf"), &coords)?;
//!
//! // Shape text
//! let shaper = TextShaper::new();
//! let shaped = shaper.shape(&font, "Hello", 100.0, Utf8Path::new("font.ttf").as_std_path())?;
//!
//! // Rasterize
//! let rasterizer = GlyphRasterizer::new();
//! let pixels = rasterizer.render_text(&font, &shaped, 3000, 1200, 0.0, Utf8Path::new("font.ttf").as_std_path())?;
//!
//! // Generate PGM
//! let pgm = ImageOutput::write_pgm_binary(&pixels, 3000, 1200)?;
//! let base64 = ImageOutput::encode_base64(&pgm);
//! # Ok::<(), haforu::Error>(())
//! ```

pub mod batch;
pub mod error;
pub mod fonts;
pub mod output;
pub mod render;
pub mod security;
pub mod shaping;

// Python bindings (optional feature)
#[cfg(feature = "python")]
pub mod python;

// Re-export main types
pub use batch::{Job, JobResult, JobSpec, RenderingOutput, TimingInfo};
pub use error::{Error, Result};
pub use fonts::{CacheStats, FontInstance, FontLoader};
pub use output::ImageOutput;
pub use render::GlyphRasterizer;
pub use shaping::{ShapedText, TextShaper};

/// Execution options for processing jobs.
#[derive(Clone, Debug, Default)]
pub struct ExecutionOptions {
    /// Optional base directory to constrain font paths.
    pub base_dir: Option<camino::Utf8PathBuf>,
    /// Optional per-job timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

/// Process a single job and return the result.
///
/// This is the main entry point for batch processing.
pub fn process_job(job: &Job, font_loader: &FontLoader) -> JobResult {
    process_job_with_options(job, font_loader, &ExecutionOptions::default())
}

/// Process a single job with execution options and return the result.
pub fn process_job_with_options(
    job: &Job,
    font_loader: &FontLoader,
    opts: &ExecutionOptions,
) -> JobResult {
    use std::time::Instant;

    let start = Instant::now();
    let timeout_guard = opts
        .timeout_ms
        .map(|ms| crate::security::TimeoutGuard::new(std::time::Duration::from_millis(ms)));

    let result = (|| -> Result<RenderingOutput> {
        if let Some(ref guard) = timeout_guard {
            guard.check("start")?;
        }
        // Load font with variations
        // Sanitize path if a base_dir is specified
        let font_path = if let Some(base) = opts.base_dir.as_ref() {
            crate::security::sanitize_path(&job.font.path, Some(base.as_path()))?
        } else {
            job.font.path.clone()
        };
        let font_instance = font_loader.load_font(&font_path, &job.font.variations)?;

        // Shape text
        let shaper = TextShaper::new();
        let shaped = shaper.shape(
            &font_instance,
            &job.text.content,
            job.font.size as f32,
            font_path.as_std_path(),
        )?;

        if let Some(ref guard) = timeout_guard {
            guard.check("shape")?;
        }
        // Rasterize
        let rasterizer = GlyphRasterizer::new();
        let pixels = rasterizer.render_text(
            &font_instance,
            &shaped,
            job.rendering.width,
            job.rendering.height,
            0.0, // No tracking for now
            font_path.as_std_path(),
        )?;

        // Calculate bounding box
        let bbox =
            GlyphRasterizer::calculate_bbox(&pixels, job.rendering.width, job.rendering.height);

        if let Some(ref guard) = timeout_guard {
            guard.check("render")?;
        }
        // Generate output image
        let image_data = match job.rendering.format.as_str() {
            "pgm" => {
                ImageOutput::write_pgm_binary(&pixels, job.rendering.width, job.rendering.height)?
            }
            "png" => ImageOutput::write_png(&pixels, job.rendering.width, job.rendering.height)?,
            _ => {
                return Err(Error::InvalidRenderParams {
                    reason: format!("Unsupported output format: {}", job.rendering.format),
                })
            }
        };

        // Base64 encode
        let base64_data = ImageOutput::encode_base64(&image_data);

        Ok(RenderingOutput {
            format: job.rendering.format.clone(),
            encoding: "base64".to_string(),
            data: base64_data,
            width: job.rendering.width,
            height: job.rendering.height,
            actual_bbox: bbox,
        })
    })();

    let elapsed = start.elapsed();

    match result {
        Ok(output) => JobResult {
            id: job.id.clone(),
            status: "success".to_string(),
            rendering: Some(output),
            error: None,
            timing: TimingInfo {
                shape_ms: 0.0, // TODO: Instrument individual stages
                render_ms: 0.0,
                total_ms: elapsed.as_secs_f64() * 1000.0,
            },
            memory: None,
        },
        Err(e) => JobResult {
            id: job.id.clone(),
            status: "error".to_string(),
            rendering: None,
            error: Some(e.to_string()),
            timing: TimingInfo {
                shape_ms: 0.0,
                render_ms: 0.0,
                total_ms: elapsed.as_secs_f64() * 1000.0,
            },
            memory: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_structure() {
        // Verify all modules are accessible
        let _ = batch::JobSpec {
            version: "1.0".to_string(),
            jobs: vec![],
        };
        let _ = FontLoader::new(512);
        let _ = TextShaper::new();
        let _ = GlyphRasterizer::new();
    }
}
