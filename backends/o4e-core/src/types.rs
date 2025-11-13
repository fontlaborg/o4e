// this_file: backends/o4e-core/src/types.rs

//! Core types used throughout the o4e rendering engine.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Font specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Font {
    /// Font family name or path to font file
    pub family: String,
    /// Font size in pixels
    pub size: f32,
    /// Font weight (100-900)
    pub weight: u16,
    /// Font style (normal, italic, oblique)
    pub style: FontStyle,
    /// Variable font axes
    pub variations: HashMap<String, f32>,
    /// OpenType features
    pub features: Features,
}

impl Font {
    pub fn new(family: impl Into<String>, size: f32) -> Self {
        Self {
            family: family.into(),
            size,
            weight: 400,
            style: FontStyle::Normal,
            variations: HashMap::new(),
            features: Features::default(),
        }
    }
}

/// Font style
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Text run - a contiguous segment of text with uniform properties
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextRun {
    /// The text content
    pub text: String,
    /// Byte range in original text
    pub range: (usize, usize),
    /// Script (ISO 15924 code)
    pub script: String,
    /// Language (BCP-47 tag)
    pub language: String,
    /// Text direction
    pub direction: Direction,
    /// Font to use for this run
    pub font: Option<Font>,
}

/// Text direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
    Auto,
}

/// Result of text shaping
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShapingResult {
    /// Positioned glyphs
    pub glyphs: Vec<Glyph>,
    /// Total advance width
    pub advance: f32,
    /// Bounding box
    pub bbox: BoundingBox,
    /// Font used for shaping (optional, for rendering)
    pub font: Option<Font>,
}

/// Individual glyph information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Glyph {
    /// Glyph ID in the font
    pub id: u32,
    /// Unicode cluster index
    pub cluster: u32,
    /// X position
    pub x: f32,
    /// Y position
    pub y: f32,
    /// Horizontal advance
    pub advance: f32,
}

/// Bounding box
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Render output
#[derive(Debug, Clone)]
pub enum RenderOutput {
    /// Bitmap image data (raw RGBA)
    Bitmap(Bitmap),
    /// SVG string
    Svg(String),
    /// PNG encoded image
    Png(Vec<u8>),
    /// Raw pixel data
    Raw(Vec<u8>),
}

/// Bitmap image
#[derive(Debug, Clone)]
pub struct Bitmap {
    /// Width in pixels
    pub width: u32,
    /// Height in pixels
    pub height: u32,
    /// Pixel data (RGBA)
    pub data: Vec<u8>,
}

/// Options for text segmentation
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SegmentOptions {
    /// Enable font fallback segmentation
    pub font_fallback: bool,
    /// Enable script itemization
    pub script_itemize: bool,
    /// Enable bidirectional analysis
    pub bidi_resolve: bool,
    /// Default language
    pub language: Option<String>,
}

/// Options for rendering
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RenderOptions {
    /// Output format
    pub format: RenderFormat,
    /// Text color (hex or rgb)
    pub color: String,
    /// Background color
    pub background: String,
    /// Antialiasing mode
    pub antialias: AntialiasMode,
    /// Hinting mode
    pub hinting: HintingMode,
    /// DPI for scaling
    pub dpi: f32,
    /// Padding around text
    pub padding: u32,
}

/// Output format for rendering
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RenderFormat {
    /// Raw RGBA bitmap
    Raw,
    /// PNG encoded image
    Png,
    /// SVG vector graphics
    Svg,
}

impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            format: RenderFormat::Raw,
            color: "#000000".to_string(),
            background: "transparent".to_string(),
            antialias: AntialiasMode::Subpixel,
            hinting: HintingMode::Slight,
            dpi: 72.0,
            padding: 10,
        }
    }
}

/// Antialiasing mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AntialiasMode {
    None,
    Grayscale,
    Subpixel,
}

/// Hinting mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HintingMode {
    None,
    Slight,
    Full,
}

/// SVG rendering options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SvgOptions {
    /// Include glyph path data
    pub include_paths: bool,
    /// Simplify paths
    pub simplify: bool,
    /// Decimal precision
    pub precision: usize,
}

impl Default for SvgOptions {
    fn default() -> Self {
        Self {
            include_paths: true,
            simplify: true,
            precision: 2,
        }
    }
}

/// OpenType features
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Features {
    /// Feature tags and their enabled state
    pub tags: HashMap<String, bool>,
}

impl Features {
    /// Create with common features enabled
    pub fn common() -> Self {
        let mut tags = HashMap::new();
        tags.insert("kern".to_string(), true);
        tags.insert("liga".to_string(), true);
        Self { tags }
    }
}
