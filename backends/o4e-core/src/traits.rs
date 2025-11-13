// this_file: backends/o4e-core/src/traits.rs

//! Core traits that all backends must implement.

use crate::types::*;
use crate::Result;

/// Main backend trait - all backends must implement this
pub trait Backend: Send + Sync {
    /// Segment text into runs for rendering
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>>;

    /// Shape a text run into glyphs
    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult>;

    /// Render shaped glyphs to output
    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput>;

    /// Backend name for identification
    fn name(&self) -> &str;

    /// Clear any internal caches
    fn clear_cache(&self);
}

/// Text segmentation trait
pub trait TextSegmenter: Send + Sync {
    /// Segment text into runs based on script, direction, and other properties
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>>;
}

/// Font shaping trait
pub trait FontShaper: Send + Sync {
    /// Shape text into positioned glyphs
    fn shape(&self, text: &str, font: &Font, features: &Features) -> Result<ShapingResult>;
}

/// Glyph rendering trait
pub trait GlyphRenderer: Send + Sync {
    /// Render glyphs to a bitmap
    fn render_to_bitmap(&self, glyphs: &[Glyph], options: &RenderOptions) -> Result<Bitmap>;

    /// Render glyphs to SVG
    fn render_to_svg(&self, glyphs: &[Glyph], options: &SvgOptions) -> Result<String>;
}
