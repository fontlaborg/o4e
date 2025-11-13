// this_file: src/python/batch.rs

//! Batch mode Python bindings.
//!
//! This module provides the `process_jobs()` function for parallel batch processing
//! of rendering jobs with streaming JSONL results.

use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use std::sync::mpsc;
use std::thread;

use crate::batch::{JobResult, JobSpec};
use crate::fonts::FontLoader;
use crate::process_job;

/// Process a batch of rendering jobs in parallel.
///
/// # Arguments
///
/// * `spec_json` - JSON string containing JobSpec with jobs array
///
/// # Returns
///
/// Iterator yielding JSONL result strings (one per completed job)
///
/// # Raises
///
/// * `ValueError` - Invalid JSON or job specification
/// * `RuntimeError` - Font loading or rendering errors
///
/// # Example
///
/// ```python
/// import haforu
/// import json
///
/// spec = {
///     "version": "1.0",
///     "jobs": [{
///         "id": "test1",
///         "font": {"path": "/path/to/font.ttf", "size": 1000, "variations": {}},
///         "text": {"content": "a", "script": "Latn"},
///         "rendering": {"format": "pgm", "encoding": "base64", "width": 3000, "height": 1200}
///     }]
/// }
///
/// for result_json in haforu.process_jobs(json.dumps(spec)):
///     result = json.loads(result_json)
///     print(f"Job {result['id']}: {result['status']}")
/// ```
#[pyfunction]
pub fn process_jobs(spec_json: &str) -> PyResult<ProcessJobsIterator> {
    // First, parse into generic JSON to validate top-level fields without requiring full schema.
    let v: serde_json::Value = serde_json::from_str(spec_json)
        .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;

    // Validate version before strict deserialization so we surface version errors clearly.
    let version = v
        .get("version")
        .and_then(|vv| vv.as_str())
        .ok_or_else(|| PyValueError::new_err("Missing 'version' field"))?;

    if version != "1.0" {
        return Err(PyValueError::new_err(format!(
            "Unsupported version: {}, expected 1.0",
            version
        )));
    }

    // Now strictly deserialize into JobSpec to validate structure.
    let spec: JobSpec = serde_json::from_value(v)
        .map_err(|e| PyValueError::new_err(format!("Invalid JSON: {}", e)))?;

    // Validate jobs
    if spec.jobs.is_empty() {
        return Err(PyValueError::new_err("Job list is empty"));
    }

    // Create iterator that yields results as they complete
    Ok(ProcessJobsIterator::new(spec))
}

/// Iterator that processes jobs in parallel and yields results.
///
/// Uses a background thread with rayon for parallel processing.
/// Results are sent via a channel and yielded as they complete.
#[pyclass]
pub struct ProcessJobsIterator {
    receiver: mpsc::Receiver<String>,
    #[allow(dead_code)]
    handle: Option<thread::JoinHandle<()>>,
}

impl std::fmt::Debug for ProcessJobsIterator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProcessJobsIterator")
            .field("receiver", &"<mpsc::Receiver>")
            .field("handle", &"<Option<JoinHandle>>")
            .finish()
    }
}

impl ProcessJobsIterator {
    fn new(spec: JobSpec) -> Self {
        let (tx, rx) = mpsc::channel();

        // Spawn background thread for parallel processing
        let handle = thread::spawn(move || {
            // Create font loader (512 cache size)
            let font_loader = FontLoader::new(512);

            // Process jobs sequentially for now
            // TODO: Use rayon for true parallel processing
            for job in spec.jobs {
                let result = process_job(&job, &font_loader);
                let result_json = serde_json::to_string(&result).unwrap_or_else(|e| {
                    serde_json::to_string(&JobResult {
                        id: job.id.clone(),
                        status: "error".to_string(),
                        rendering: None,
                        error: Some(format!("Failed to serialize result: {}", e)),
                        timing: crate::batch::TimingInfo {
                            shape_ms: 0.0,
                            render_ms: 0.0,
                            total_ms: 0.0,
                        },
                        memory: None,
                    })
                    .unwrap()
                });

                // Send result (ignore error if receiver dropped)
                let _ = tx.send(result_json);
            }
        });

        Self {
            receiver: rx,
            handle: Some(handle),
        }
    }
}

#[pymethods]
impl ProcessJobsIterator {
    fn __iter__(slf: PyRef<Self>) -> PyRef<Self> {
        slf
    }

    fn __next__(&mut self) -> Option<String> {
        self.receiver.recv().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_process_jobs_empty() {
        let spec_json = r#"{"version": "1.0", "jobs": []}"#;
        let result = process_jobs(spec_json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_process_jobs_invalid_json() {
        let spec_json = "not valid json";
        let result = process_jobs(spec_json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_process_jobs_invalid_version() {
        let spec_json = r#"{"version": "2.0", "jobs": [{"id": "test"}]}"#;
        let result = process_jobs(spec_json);
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Unsupported version"));
    }
}
