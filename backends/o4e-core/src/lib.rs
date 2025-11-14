// this_file: backends/o4e-core/src/lib.rs

//! Core traits and types for the o4e text rendering engine.

pub mod cache;
pub mod diagnostics;
pub mod error;
pub mod surface;
pub mod traits;
pub mod types;
pub mod utils;

pub use cache::FontCache;
pub use diagnostics::RenderOptionsDiagnostics;
pub use error::O4eError;
pub use surface::{RenderSurface, SurfaceFormat};
pub use traits::{Backend, FontShaper, GlyphRenderer, TextSegmenter};
pub use types::{
    Bitmap, Features, Font, Glyph, RenderFormat, RenderOptions, RenderOutput, SegmentOptions,
    ShapingResult, SvgOptions, TextRun,
};

/// Result type for o4e operations
pub type Result<T> = std::result::Result<T, O4eError>;
