// this_file: crates/o4e-render/src/lib.rs

//! Rendering utilities for o4e text engine.

pub mod batch;
pub mod outlines;
pub mod perf;
pub mod svg;

pub use batch::{BatchItem, BatchRenderer, BatchResult};
pub use outlines::{glyph_outline, GlyphOutline, OutlineCommand};
pub use perf::{BufferPool, MetricType, PerfMetrics, PerfScope, PerfStats};
pub use svg::SvgRenderer;
