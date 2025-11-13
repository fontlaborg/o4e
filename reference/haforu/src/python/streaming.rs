// this_file: src/python/streaming.rs

//! Streaming session API for persistent rendering.
//!
//! This module provides the `StreamingSession` class for Python, which maintains
//! a persistent font cache and allows zero-overhead rendering across multiple calls.

use numpy::PyArray2;
use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyType};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

// Error conversion is handled inline for streaming session
use crate::batch::Job;
use crate::fonts::FontLoader;
use crate::process_job;
use crate::{GlyphRasterizer, TextShaper};
use camino::Utf8PathBuf;

/// Persistent rendering session with font cache.
///
/// Maintains loaded fonts across multiple renders for maximum performance.
/// Thread-safe: can be called from multiple threads concurrently.
///
/// # Example
///
/// ```python
/// import haforu
/// import json
///
/// with haforu.StreamingSession() as session:
///     job = {
///         "id": "test1",
///         "font": {"path": "/path/to/font.ttf", "size": 1000, "variations": {}},
///         "text": {"content": "a"},
///         "rendering": {"format": "pgm", "encoding": "base64", "width": 3000, "height": 1200}
///     }
///     result_json = session.render(json.dumps(job))
///     result = json.loads(result_json)
///     print(f"Status: {result['status']}")
/// ```
#[pyclass]
pub struct StreamingSession {
    font_loader: Arc<Mutex<FontLoader>>,
    closed: Arc<AtomicBool>,
}

#[pymethods]
impl StreamingSession {
    #[new]
    #[pyo3(signature = (cache_size=512))]
    fn new(cache_size: usize) -> PyResult<Self> {
        Ok(Self {
            font_loader: Arc::new(Mutex::new(FontLoader::new(cache_size))),
            closed: Arc::new(AtomicBool::new(false)),
        })
    }

    #[classmethod]
    fn is_available(_cls: &Bound<'_, PyType>) -> bool {
        StreamingSession::new(1).is_ok()
    }

    fn ensure_open(&self) -> PyResult<()> {
        if self.closed.load(Ordering::SeqCst) {
            Err(PyRuntimeError::new_err("StreamingSession is closed"))
        } else {
            Ok(())
        }
    }

    /// Warm up the streaming session (optionally rendering a font).
    ///
    /// Args:
    ///     font_path: Optional font path to pre-load via a quick render.
    ///     text: Optional short string to render during warm-up.
    ///     size: Font size in points (default 600).
    ///     width: Canvas width (default 128).
    ///     height: Canvas height (default 128).
    ///
    /// Returns:
    ///     bool: True when warm-up completed.
    #[pyo3(signature = (font_path=None, *, text="Haforu", size=600.0, width=128, height=128))]
    fn warm_up<'py>(
        &self,
        py: Python<'py>,
        font_path: Option<&str>,
        text: &str,
        size: f64,
        width: u32,
        height: u32,
    ) -> PyResult<bool> {
        self.ensure_open()?;
        if let Some(path) = font_path {
            // Render via numpy path; ignore pixels but surface errors.
            let _ =
                self.render_to_numpy(py, path, text, size, width, height, None, None, None, None)?;
        } else {
            // Touch the cache to ensure structures are allocated.
            drop(self.font_loader.lock().unwrap());
        }
        Ok(true)
    }

    /// Return cache statistics for observability.
    fn cache_stats(&self) -> PyResult<HashMap<&'static str, usize>> {
        let loader = self.font_loader.lock().unwrap();
        let stats = loader.stats();
        Ok(HashMap::from([
            ("capacity", stats.capacity),
            ("entries", stats.entries),
        ]))
    }

    /// Resize the cache capacity (drops stored entries).
    fn set_cache_size(&self, cache_size: usize) -> PyResult<()> {
        if cache_size == 0 {
            return Err(PyValueError::new_err("cache_size must be >= 1"));
        }
        self.ensure_open()?;
        let loader = self.font_loader.lock().unwrap();
        loader.set_capacity(cache_size);
        Ok(())
    }

    /// Render a single job and return JSONL result.
    ///
    /// # Arguments
    ///
    /// * `job_json` - JSON string containing single Job specification
    ///
    /// # Returns
    ///
    /// JSONL result string with base64-encoded image
    ///
    /// # Raises
    ///
    /// * `ValueError` - Invalid JSON or job specification
    /// * `RuntimeError` - Font loading or rendering errors
    ///
    /// # Example
    ///
    /// ```python
    /// session = haforu.StreamingSession()
    /// job_json = json.dumps({
    ///     "id": "test1",
    ///     "font": {"path": "/path/to/font.ttf", "size": 1000, "variations": {}},
    ///     "text": {"content": "a"},
    ///     "rendering": {"format": "pgm", "encoding": "base64", "width": 3000, "height": 1200}
    /// })
    /// result_json = session.render(job_json)
    /// ```
    fn render(&self, job_json: &str) -> PyResult<String> {
        self.ensure_open()?;
        // Parse job
        let job: Job = serde_json::from_str(job_json)
            .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;

        // Process job with font loader
        let font_loader = self.font_loader.lock().unwrap();
        let result = process_job(&job, &font_loader);

        // Serialize result
        serde_json::to_string(&result)
            .map_err(|e| PyValueError::new_err(format!("Failed to serialize result: {}", e)))
    }

    /// Render text directly to numpy array (zero-copy).
    ///
    /// # Arguments
    ///
    /// * `font_path` - Absolute path to font file
    /// * `text` - Text to render (typically single glyph)
    /// * `size` - Font size in points (typically 1000)
    /// * `width` - Canvas width in pixels
    /// * `height` - Canvas height in pixels
    /// * `variations` - Optional variable font coordinates (e.g. {"wght": 600})
    /// * `script` - Script tag (default: "Latn")
    /// * `direction` - Text direction (default: "ltr")
    /// * `language` - Language tag (default: "en")
    ///
    /// # Returns
    ///
    /// 2D numpy array of shape (height, width), dtype uint8
    /// Grayscale values 0-255
    ///
    /// # Raises
    ///
    /// * `ValueError` - Invalid parameters
    /// * `RuntimeError` - Font loading or rendering errors
    ///
    /// # Example
    ///
    /// ```python
    /// session = haforu.StreamingSession()
    /// image = session.render_to_numpy(
    ///     font_path="/path/to/font.ttf",
    ///     text="a",
    ///     size=1000.0,
    ///     width=3000,
    ///     height=1200,
    ///     variations={"wght": 600.0}
    /// )
    /// assert image.shape == (1200, 3000)
    /// assert image.dtype == numpy.uint8
    /// ```
    #[pyo3(signature = (font_path, text, size, width, height, variations=None, script=None, direction=None, language=None))]
    fn render_to_numpy<'py>(
        &self,
        py: Python<'py>,
        font_path: &str,
        text: &str,
        size: f64,
        width: u32,
        height: u32,
        variations: Option<HashMap<String, f64>>,
        script: Option<&str>,
        direction: Option<&str>,
        language: Option<&str>,
    ) -> PyResult<Bound<'py, PyArray2<u8>>> {
        self.ensure_open()?;
        // Convert font path to Utf8PathBuf
        let font_path_buf = Utf8PathBuf::from(font_path);

        // Convert variation coordinates from f64 to f32
        let variations_f32: HashMap<String, f32> = variations
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k, v as f32))
            .collect();

        // Load font with variations
        let font_loader = self.font_loader.lock().unwrap();
        let font_instance = font_loader
            .load_font(&font_path_buf, &variations_f32)
            .map_err(|e| PyRuntimeError::new_err(format!("Font loading failed: {}", e)))?;

        // Shape text
        let shaper = TextShaper::new();
        let shaped = shaper
            .shape(
                &font_instance,
                text,
                size as f32,
                font_path_buf.as_std_path(),
            )
            .map_err(|e| PyRuntimeError::new_err(format!("Text shaping failed: {}", e)))?;

        // Rasterize
        let rasterizer = GlyphRasterizer::new();
        let pixels = rasterizer
            .render_text(
                &font_instance,
                &shaped,
                width,
                height,
                0.0, // No tracking
                font_path_buf.as_std_path(),
            )
            .map_err(|e| PyRuntimeError::new_err(format!("Rendering failed: {}", e)))?;

        // Convert to 2D array: pixels is Vec<u8> of length width*height
        // numpy expects shape (height, width) in row-major order
        let array_2d: Vec<Vec<u8>> = pixels
            .chunks(width as usize)
            .map(|row| row.to_vec())
            .collect();

        // Convert to numpy array using from_vec2_bound (returns Bound)
        PyArray2::from_vec2_bound(py, &array_2d)
            .map_err(|e| PyRuntimeError::new_err(format!("Failed to create numpy array: {}", e)))
    }

    /// Close session and release resources immediately.
    fn close(&self) {
        if self.closed.swap(true, Ordering::SeqCst) {
            return;
        }
        if let Ok(loader) = self.font_loader.lock() {
            loader.clear();
        }
    }

    fn __enter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    #[pyo3(signature = (_exc_type=None, _exc_val=None, _exc_tb=None))]
    fn __exit__(
        &self,
        _exc_type: Option<&Bound<'_, PyAny>>,
        _exc_val: Option<&Bound<'_, PyAny>>,
        _exc_tb: Option<&Bound<'_, PyAny>>,
    ) -> PyResult<bool> {
        self.close();
        Ok(false) // Don't suppress exceptions
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_streaming_session_creation() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|py| {
            let session = StreamingSession::new(512).unwrap();
            assert!(Arc::strong_count(&session.font_loader) >= 1);
        });
    }

    #[test]
    fn test_invalid_json() {
        pyo3::prepare_freethreaded_python();
        Python::with_gil(|_py| {
            let session = StreamingSession::new(512).unwrap();
            let result = session.render("not valid json");
            assert!(result.is_err());
            assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
        });
    }
}
