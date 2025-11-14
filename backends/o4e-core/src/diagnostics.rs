// this_file: backends/o4e-core/src/diagnostics.rs

//! Rendering diagnostics helpers used by backends for structured debug logging.

use crate::types::{RenderOptions, ShapingResult};
use log::{debug, log_enabled, Level};

/// Lightweight snapshot of the effective render request.
#[derive(Debug)]
pub struct RenderOptionsDiagnostics<'a> {
    backend: &'a str,
    glyph_count: usize,
    format: &'a str,
    antialias: &'a str,
    hinting: &'a str,
    color: &'a str,
    background: &'a str,
    dpi: f32,
    padding: u32,
    font: Option<&'a str>,
}

impl<'a> RenderOptionsDiagnostics<'a> {
    /// Capture the diagnostic snapshot for the provided backend/render call.
    pub fn new(backend: &'a str, shaped: &'a ShapingResult, options: &'a RenderOptions) -> Self {
        Self {
            backend,
            glyph_count: shaped.glyphs.len(),
            format: match options.format {
                crate::types::RenderFormat::Raw => "raw",
                crate::types::RenderFormat::Png => "png",
                crate::types::RenderFormat::Svg => "svg",
            },
            antialias: match options.antialias {
                crate::types::AntialiasMode::None => "none",
                crate::types::AntialiasMode::Grayscale => "grayscale",
                crate::types::AntialiasMode::Subpixel => "subpixel",
            },
            hinting: match options.hinting {
                crate::types::HintingMode::None => "none",
                crate::types::HintingMode::Slight => "slight",
                crate::types::HintingMode::Full => "full",
            },
            color: options.color.as_str(),
            background: options.background.as_str(),
            dpi: options.dpi,
            padding: options.padding,
            font: shaped.font.as_ref().map(|font| font.family.as_str()),
        }
    }

    /// Emit the diagnostic snapshot at debug level when logging is enabled.
    pub fn log(&self) {
        if log_enabled!(Level::Debug) {
            debug!(
                target: "o4e::render",
                "backend={backend} format={format} glyphs={glyphs} aa={aa} hinting={hinting} dpi={dpi:.1} padding={padding} color={color} background={background} font={font}",
                backend = self.backend,
                format = self.format,
                glyphs = self.glyph_count,
                aa = self.antialias,
                hinting = self.hinting,
                dpi = self.dpi,
                padding = self.padding,
                color = self.color,
                background = self.background,
                font = self.font.unwrap_or("<unknown>"),
            );
        }
    }
}
