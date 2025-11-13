// this_file: crates/o4e-render/src/batch.rs

//! Batch rendering implementation for parallel text processing.

use o4e_core::{Backend, Font, RenderOptions, RenderOutput, Result, SegmentOptions, ShapingResult};
use rayon::iter::IndexedParallelIterator;
use rayon::prelude::*;
use std::sync::Arc;

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
        items
            .into_par_iter()
            .enumerate()
            .map(|(index, item)| {
                let result = self.render_single(&item);
                BatchResult { index, result }
            })
            .collect()
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

        pool.install(|| self.render_batch(items))
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
            let result = self.render_single(&item);
            BatchResult { index, result }
        })
    }
}

/// Combine multiple shaped results into one.
fn combine_shaped_results(results: Vec<ShapingResult>) -> ShapingResult {
    if results.is_empty() {
        return ShapingResult {
            glyphs: vec![],
            advance: 0.0,
            bbox: o4e_core::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: 0.0,
            },
            font: None,
        };
    }

    if results.len() == 1 {
        return results.into_iter().next().unwrap();
    }

    let mut all_glyphs = Vec::new();
    let mut total_advance = 0.0;
    let mut x_offset = 0.0;

    for result in results {
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
        glyphs: all_glyphs,
        advance: total_advance,
        bbox,
        font: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_combine_empty_results() {
        let combined = combine_shaped_results(vec![]);
        assert!(combined.glyphs.is_empty());
        assert_eq!(combined.advance, 0.0);
    }

    #[test]
    fn test_combine_single_result() {
        let result = ShapingResult {
            glyphs: vec![],
            advance: 10.0,
            bbox: o4e_core::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 10.0,
                height: 20.0,
            },
            font: None,
        };

        let combined = combine_shaped_results(vec![result.clone()]);
        assert_eq!(combined.advance, result.advance);
    }
}
