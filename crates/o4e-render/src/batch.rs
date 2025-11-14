// this_file: crates/o4e-render/src/batch.rs

//! Batch rendering implementation for parallel text processing.

use hdrhistogram::Histogram;
use o4e_core::{Backend, Font, RenderOptions, RenderOutput, Result, SegmentOptions, ShapingResult};
use parking_lot::Mutex;
use rayon::iter::IndexedParallelIterator;
use rayon::prelude::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

/// Item to be rendered in batch.
#[derive(Clone)]
pub struct BatchItem {
    /// Text to render
    pub text: String,
    /// Font specification
    pub font: Font,
    /// Segmentation options
    pub segment_options: SegmentOptions,
    /// Render options
    pub render_options: RenderOptions,
}

/// Result from batch rendering.
pub struct BatchResult {
    /// Index of the item in the batch
    pub index: usize,
    /// Rendering result or error
    pub result: Result<RenderOutput>,
    /// How long the rendering took
    pub elapsed: Duration,
}

/// Batch renderer for parallel text rendering.
pub struct BatchRenderer {
    backend: Arc<dyn Backend>,
}

impl BatchRenderer {
    /// Create a new batch renderer with the given backend.
    pub fn new(backend: Arc<dyn Backend>) -> Self {
        Self { backend }
    }

    /// Render a batch of items in parallel.
    pub fn render_batch(&self, items: Vec<BatchItem>) -> Vec<BatchResult> {
        self.render_batch_internal(items, None)
    }

    /// Render a batch of items while reporting progress.
    pub fn render_batch_with_progress<F>(
        &self,
        items: Vec<BatchItem>,
        progress: F,
    ) -> Vec<BatchResult>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        self.render_batch_internal(items, Some(Arc::new(progress)))
    }

    /// Render a batch with a specific number of threads.
    pub fn render_batch_with_threads(
        &self,
        items: Vec<BatchItem>,
        num_threads: usize,
    ) -> Vec<BatchResult> {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        pool.install(|| self.render_batch_internal(items, None))
    }

    /// Render a batch with a specific number of threads and progress reporting.
    pub fn render_batch_with_threads_and_progress<F>(
        &self,
        items: Vec<BatchItem>,
        num_threads: usize,
        progress: F,
    ) -> Vec<BatchResult>
    where
        F: Fn(ProgressUpdate) + Send + Sync + 'static,
    {
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(num_threads)
            .build()
            .unwrap();

        pool.install(|| self.render_batch_internal(items, Some(Arc::new(progress))))
    }

    /// Render a single item.
    fn render_single(&self, item: &BatchItem) -> Result<RenderOutput> {
        // 1. Segment text
        let runs = self.backend.segment(&item.text, &item.segment_options)?;

        // 2. Shape each run
        let mut shaped_results = Vec::new();
        for run in runs {
            let shaped = self.backend.shape(&run, &item.font)?;
            shaped_results.push(shaped);
        }

        // 3. Combine shaped results
        let combined = combine_shaped_results(shaped_results);

        // 4. Render
        self.backend.render(&combined, &item.render_options)
    }

    /// Process items from an indexed iterator in parallel.
    pub fn render_streaming<'a, I>(
        &'a self,
        items: I,
    ) -> impl ParallelIterator<Item = BatchResult> + 'a
    where
        I: IndexedParallelIterator<Item = BatchItem> + 'a,
    {
        items.enumerate().map(move |(index, item)| {
            let start = Instant::now();
            let result = self.render_single(&item);
            let elapsed = start.elapsed();
            BatchResult {
                index,
                result,
                elapsed,
            }
        })
    }

    fn render_batch_internal(
        &self,
        items: Vec<BatchItem>,
        progress: Option<Arc<dyn Fn(ProgressUpdate) + Send + Sync>>,
    ) -> Vec<BatchResult> {
        if items.is_empty() {
            return Vec::new();
        }

        let total = items.len();
        let metrics = Arc::new(BatchMetrics::new());
        let state = progress.map(|callback| {
            Arc::new(ProgressState {
                callback,
                counter: Arc::new(AtomicUsize::new(0)),
                total,
                metrics: metrics.clone(),
            })
        });

        items
            .into_par_iter()
            .enumerate()
            .map({
                let state = state.clone();
                move |(index, item)| {
                    let start = Instant::now();
                    let result = self.render_single(&item);
                    let elapsed = start.elapsed();
                    metrics.record(elapsed);
                    if let Some(state) = state.as_ref() {
                        let current = state.counter.fetch_add(1, Ordering::SeqCst) + 1;
                        state.report(current);
                    }
                    BatchResult {
                        index,
                        result,
                        elapsed,
                    }
                }
            })
            .collect()
    }
}

struct ProgressState {
    callback: Arc<dyn Fn(ProgressUpdate) + Send + Sync>,
    counter: Arc<AtomicUsize>,
    total: usize,
    metrics: Arc<BatchMetrics>,
}

impl ProgressState {
    fn report(&self, completed: usize) {
        let snapshot = self.metrics.snapshot();
        (self.callback)(ProgressUpdate {
            completed,
            total: self.total,
            p50: snapshot.p50,
            p90: snapshot.p90,
            p99: snapshot.p99,
        });
    }
}

struct BatchMetrics {
    histogram: Mutex<Histogram<u64>>,
}

impl BatchMetrics {
    fn new() -> Self {
        let histogram =
            Histogram::new_with_bounds(1, 60_000_000, 3).expect("histogram bounds should be valid"); // up to 60s per item
        Self {
            histogram: Mutex::new(histogram),
        }
    }

    fn record(&self, duration: Duration) {
        let micros = duration.as_micros().clamp(1, u64::MAX as u128) as u64;
        let mut histogram = self.histogram.lock();
        let _ = histogram.record(micros);
    }

    fn snapshot(&self) -> LatencySnapshot {
        let histogram = self.histogram.lock();
        if histogram.len() == 0 {
            return LatencySnapshot::default();
        }
        LatencySnapshot {
            p50: micros_to_duration(histogram.value_at_percentile(50.0)),
            p90: micros_to_duration(histogram.value_at_percentile(90.0)),
            p99: micros_to_duration(histogram.value_at_percentile(99.0)),
        }
    }
}

#[derive(Default)]
struct LatencySnapshot {
    p50: Duration,
    p90: Duration,
    p99: Duration,
}

fn micros_to_duration(value: u64) -> Duration {
    Duration::from_micros(value.max(1))
}

/// Latency-aware update reported to progress callbacks.
pub struct ProgressUpdate {
    pub completed: usize,
    pub total: usize,
    pub p50: Duration,
    pub p90: Duration,
    pub p99: Duration,
}

/// Combine multiple shaped results into one.
fn combine_shaped_results(results: Vec<ShapingResult>) -> ShapingResult {
    if results.is_empty() {
        return ShapingResult {
            text: String::new(),
            glyphs: vec![],
            advance: 0.0,
            bbox: o4e_core::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            font: None,
            direction: o4e_core::types::Direction::LeftToRight,
        };
    }

    if results.len() == 1 {
        return results.into_iter().next().unwrap();
    }

    let mut all_glyphs = Vec::new();
    let mut total_advance = 0.0;
    let mut x_offset = 0.0;
    let mut combined_text = String::new();

    for result in results {
        if !result.text.is_empty() {
            combined_text.push_str(&result.text);
        }
        // Offset glyphs by accumulated advance
        for mut glyph in result.glyphs {
            glyph.x += x_offset;
            all_glyphs.push(glyph);
        }
        total_advance += result.advance;
        x_offset += result.advance;
    }

    let bbox = o4e_core::utils::calculate_bbox(&all_glyphs);

    ShapingResult {
        text: combined_text,
        glyphs: all_glyphs,
        advance: total_advance,
        bbox,
        font: None,
        direction: o4e_core::types::Direction::LeftToRight,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use o4e_core::{
        types::{BoundingBox, Direction, Glyph, TextRun},
        Result,
    };
    use std::sync::atomic::Ordering;

    #[derive(Default)]
    struct DummyBackend;

    impl Backend for DummyBackend {
        fn segment(&self, text: &str, _options: &SegmentOptions) -> Result<Vec<TextRun>> {
            Ok(vec![TextRun {
                text: text.to_string(),
                range: (0, text.len()),
                script: "Latin".to_string(),
                language: "en".to_string(),
                direction: Direction::LeftToRight,
                font: None,
            }])
        }

        fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
            let glyphs: Vec<Glyph> = run
                .text
                .char_indices()
                .map(|(idx, _)| Glyph {
                    id: idx as u32,
                    cluster: idx as u32,
                    x: idx as f32,
                    y: 0.0,
                    advance: 1.0,
                })
                .collect();

            let bbox = if glyphs.is_empty() {
                BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 0.0,
                }
            } else {
                o4e_core::utils::calculate_bbox(&glyphs)
            };

            Ok(ShapingResult {
                text: run.text.clone(),
                advance: glyphs.len() as f32,
                glyphs,
                bbox,
                font: Some(font.clone()),
                direction: run.direction,
            })
        }

        fn render(&self, shaped: &ShapingResult, _options: &RenderOptions) -> Result<RenderOutput> {
            Ok(RenderOutput::Raw(vec![0; shaped.glyphs.len().max(1)]))
        }

        fn name(&self) -> &str {
            "dummy"
        }

        fn clear_cache(&self) {}
    }

    fn make_items(count: usize) -> Vec<BatchItem> {
        let font = Font::new("Test", 12.0);
        let render_options = RenderOptions::default();
        let segment_options = SegmentOptions::default();
        (0..count)
            .map(|i| BatchItem {
                text: format!("Item {}", i),
                font: font.clone(),
                segment_options: segment_options.clone(),
                render_options: render_options.clone(),
            })
            .collect()
    }

    #[test]
    fn test_combine_empty_results() {
        let combined = combine_shaped_results(vec![]);
        assert!(combined.glyphs.is_empty());
        assert_eq!(combined.advance, 0.0);
        assert!(combined.text.is_empty());
    }

    #[test]
    fn test_combine_single_result() {
        let result = ShapingResult {
            text: "abc".to_string(),
            glyphs: vec![],
            advance: 10.0,
            bbox: o4e_core::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
            },
            font: None,
            direction: o4e_core::types::Direction::LeftToRight,
        };

        let combined = combine_shaped_results(vec![result.clone()]);
        assert_eq!(combined.advance, result.advance);
        assert_eq!(combined.text, "abc".to_string());
    }

    #[test]
    fn test_progress_callback_receives_updates() {
        let renderer = BatchRenderer::new(Arc::new(DummyBackend::default()));
        let items = make_items(5);
        let invocations = Arc::new(AtomicUsize::new(0));

        renderer.render_batch_with_progress(items, {
            let invocations = invocations.clone();
            move |update| {
                assert!(update.completed <= update.total);
                invocations.fetch_add(1, Ordering::SeqCst);
                if update.completed > 0 {
                    assert!(update.p50 >= Duration::ZERO);
                }
            }
        });

        assert_eq!(invocations.load(Ordering::SeqCst), 5);
    }

    fn assert_large_batch(count: usize) {
        let renderer = BatchRenderer::new(Arc::new(DummyBackend::default()));
        let results = renderer.render_batch(make_items(count));
        assert_eq!(results.len(), count);
        assert!(results.iter().all(|result| result.result.is_ok()));
        assert!(results
            .iter()
            .all(|result| result.elapsed >= Duration::ZERO));
    }

    #[test]
    fn test_render_batch_handles_100_items() {
        assert_large_batch(100);
    }

    #[test]
    fn test_render_batch_handles_1000_items() {
        assert_large_batch(1_000);
    }

    #[test]
    fn test_render_batch_handles_10000_items() {
        assert_large_batch(10_000);
    }

    #[test]
    fn test_combined_glyphs_have_monotonic_offsets() {
        let renderer = DummyBackend::default();
        let font = Font::new("Test", 12.0);
        let run = TextRun {
            text: "abc".into(),
            range: (0, 3),
            script: "Latn".into(),
            language: "en".into(),
            direction: Direction::LeftToRight,
            font: Some(font.clone()),
        };

        let shaped = renderer.shape(&run, &font).unwrap();
        let combined = super::combine_shaped_results(vec![shaped.clone(), shaped]);
        let mut last_x = -f32::INFINITY;
        for glyph in combined.glyphs {
            assert!(
                glyph.x >= last_x,
                "glyph positions must be monotonic to preserve layout"
            );
            last_x = glyph.x;
        }
    }
}
