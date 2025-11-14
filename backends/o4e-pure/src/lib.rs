// this_file: backends/o4e-pure/src/lib.rs

//! Pure Rust backend for o4e - suitable for WASM and no-std environments.

extern crate alloc;

use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use o4e_core::{
    types::{Direction, RenderFormat},
    Backend, Bitmap, Font, Glyph, RenderOptions, RenderOutput, RenderSurface, Result,
    SegmentOptions, ShapingResult, TextRun,
};

/// Pure Rust backend using rustybuzz for shaping and tiny-skia for rendering
pub struct PureRustBackend {
    _placeholder: (),
}

impl PureRustBackend {
    pub fn new() -> Self {
        Self { _placeholder: () }
    }

    /// Simple text segmentation - breaks on script changes
    fn segment_text(&self, text: &str) -> Vec<TextRun> {
        let mut runs = Vec::new();
        let mut current_run = String::new();
        let mut current_script = None;
        let mut start = 0;

        for (idx, ch) in text.char_indices() {
            let script = detect_script(ch);

            if current_script.is_none() {
                current_script = Some(script);
            }

            if Some(script) != current_script && !current_run.is_empty() {
                // Script boundary - create new run
                runs.push(TextRun {
                    text: current_run.clone(),
                    range: (start, idx),
                    script: script_to_string(current_script.unwrap()),
                    language: String::from("en"),
                    direction: if is_rtl_script(current_script.unwrap()) {
                        Direction::RightToLeft
                    } else {
                        Direction::LeftToRight
                    },
                    font: None,
                });

                current_run.clear();
                current_script = Some(script);
                start = idx;
            }

            current_run.push(ch);
        }

        // Add final run
        if !current_run.is_empty() {
            runs.push(TextRun {
                text: current_run,
                range: (start, text.len()),
                script: script_to_string(current_script.unwrap_or(Script::Latin)),
                language: String::from("en"),
                direction: if is_rtl_script(current_script.unwrap_or(Script::Latin)) {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                },
                font: None,
            });
        }

        if runs.is_empty() {
            // Empty text - return single empty run
            runs.push(TextRun {
                text: String::new(),
                range: (0, 0),
                script: String::from("Latin"),
                language: String::from("en"),
                direction: Direction::LeftToRight,
                font: None,
            });
        }

        runs
    }

    /// Simple glyph rendering - creates basic bitmap
    fn render_glyphs(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<Bitmap> {
        let padding = options.padding as f32;
        let width = ((shaped.bbox.width + padding * 2.0).ceil() as u32).max(1);
        let height = ((shaped.bbox.height + padding * 2.0).ceil() as u32).max(1);

        let mut bitmap = vec![0u8; (width * height * 4) as usize];

        // Parse colors
        let (text_r, text_g, text_b, text_a) = parse_color(&options.color);

        // Fill background
        if options.background != "transparent" {
            let (bg_r, bg_g, bg_b, bg_a) = parse_color(&options.background);
            for pixel in bitmap.chunks_exact_mut(4) {
                pixel[0] = bg_r;
                pixel[1] = bg_g;
                pixel[2] = bg_b;
                pixel[3] = bg_a;
            }
        }

        // In a real implementation, we'd render actual glyphs here
        // For now, just create placeholder rectangles for each glyph
        for glyph in &shaped.glyphs {
            let glyph_width = glyph.advance.min(10.0) as u32;
            let glyph_height = 20.min(height - 1);

            let x = ((glyph.x + padding) as u32).min(width.saturating_sub(glyph_width));
            let y = (padding as u32).min(height.saturating_sub(glyph_height));

            // Draw glyph rectangle
            for dy in 0..glyph_height {
                for dx in 0..glyph_width {
                    let px = (x + dx) as usize;
                    let py = (y + dy) as usize;
                    if px < width as usize && py < height as usize {
                        let idx = (py * width as usize + px) * 4;
                        if idx + 3 < bitmap.len() {
                            bitmap[idx] = text_r;
                            bitmap[idx + 1] = text_g;
                            bitmap[idx + 2] = text_b;
                            bitmap[idx + 3] = text_a;
                        }
                    }
                }
            }
        }

        Ok(Bitmap {
            width,
            height,
            data: bitmap,
        })
    }
}

impl Backend for PureRustBackend {
    fn segment(&self, text: &str, _options: &SegmentOptions) -> Result<Vec<TextRun>> {
        Ok(self.segment_text(text))
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // Simple shaping - one glyph per character
        let mut glyphs = Vec::new();
        let mut x_offset = 0.0;

        // Estimate glyph advance based on font size
        let advance = font.size * 0.6; // Rough approximation

        for (idx, ch) in run.text.char_indices() {
            glyphs.push(Glyph {
                id: ch as u32,
                cluster: idx as u32,
                x: x_offset,
                y: 0.0,
                advance,
            });
            x_offset += advance;
        }

        let bbox = if glyphs.is_empty() {
            o4e_core::types::BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 0.0,
                height: font.size,
            }
        } else {
            o4e_core::types::BoundingBox {
                x: 0.0,
                y: -font.size * 0.8,
                width: x_offset,
                height: font.size,
            }
        };

        Ok(ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance: x_offset,
            bbox,
            font: Some(font.clone()),
            direction: run.direction,
        })
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // Diagnostics removed for simplicity
        match options.format {
            RenderFormat::Raw | RenderFormat::Png => {
                let bitmap = self.render_glyphs(shaped, options)?;
                let surface =
                    RenderSurface::from_rgba(bitmap.width, bitmap.height, bitmap.data, false);
                surface.into_render_output(options.format)
            }
            RenderFormat::Svg => {
                // Simple SVG generation
                let mut svg = String::from(r#"<svg xmlns="http://www.w3.org/2000/svg">"#);

                for glyph in &shaped.glyphs {
                    svg.push_str(&format!(
                        r#"<rect x="{}" y="10" width="8" height="12" fill="black"/>"#,
                        glyph.x
                    ));
                }

                svg.push_str("</svg>");
                Ok(RenderOutput::Svg(svg))
            }
        }
    }

    fn name(&self) -> &str {
        "PureRust"
    }

    fn clear_cache(&self) {
        // No cache to clear
    }
}

impl Default for PureRustBackend {
    fn default() -> Self {
        Self::new()
    }
}

// Simple script detection
#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(clippy::upper_case_acronyms)]
enum Script {
    Latin,
    Cyrillic,
    Greek,
    Arabic,
    Hebrew,
    CJK,
    Other,
}

fn detect_script(ch: char) -> Script {
    match ch as u32 {
        0x0000..=0x007F => Script::Latin,
        0x0080..=0x00FF => Script::Latin,
        0x0100..=0x017F => Script::Latin,
        0x0400..=0x04FF => Script::Cyrillic,
        0x0370..=0x03FF => Script::Greek,
        0x0600..=0x06FF => Script::Arabic,
        0x0590..=0x05FF => Script::Hebrew,
        0x4E00..=0x9FFF => Script::CJK,
        0x3040..=0x309F => Script::CJK, // Hiragana
        0x30A0..=0x30FF => Script::CJK, // Katakana
        0xAC00..=0xD7AF => Script::CJK, // Hangul
        _ => Script::Other,
    }
}

fn script_to_string(script: Script) -> String {
    match script {
        Script::Latin => String::from("Latin"),
        Script::Cyrillic => String::from("Cyrillic"),
        Script::Greek => String::from("Greek"),
        Script::Arabic => String::from("Arabic"),
        Script::Hebrew => String::from("Hebrew"),
        Script::CJK => String::from("CJK"),
        Script::Other => String::from("Other"),
    }
}

fn is_rtl_script(script: Script) -> bool {
    matches!(script, Script::Arabic | Script::Hebrew)
}

fn parse_color(color: &str) -> (u8, u8, u8, u8) {
    if color.starts_with('#') && color.len() == 7 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&color[1..3], 16),
            u8::from_str_radix(&color[3..5], 16),
            u8::from_str_radix(&color[5..7], 16),
        ) {
            return (r, g, b, 255);
        }
    }

    // Default to black
    (0, 0, 0, 255)
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen]
    pub struct WasmRenderer {
        backend: PureRustBackend,
    }

    #[wasm_bindgen]
    impl WasmRenderer {
        #[wasm_bindgen(constructor)]
        pub fn new() -> Self {
            Self {
                backend: PureRustBackend::new(),
            }
        }

        #[wasm_bindgen]
        pub fn render_text(&self, text: &str, font_size: f32) -> Vec<u8> {
            let font = Font::new("sans-serif", font_size);
            let options = RenderOptions::default();

            let runs = self
                .backend
                .segment(text, &SegmentOptions::default())
                .unwrap_or_default();

            if let Some(run) = runs.first() {
                if let Ok(shaped) = self.backend.shape(run, &font) {
                    if let Ok(RenderOutput::Bitmap(bitmap)) = self.backend.render(&shaped, &options)
                    {
                        return bitmap.data;
                    }
                }
            }

            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_rust_backend() {
        let backend = PureRustBackend::new();
        assert_eq!(backend.name(), "PureRust");
    }

    #[test]
    fn test_segmentation() {
        let backend = PureRustBackend::new();
        let text = "Hello мир";
        let runs = backend.segment(text, &SegmentOptions::default()).unwrap();
        assert_eq!(runs.len(), 2); // Latin and Cyrillic
    }

    #[test]
    fn test_simple_shaping() {
        let backend = PureRustBackend::new();
        let font = Font::new("test", 16.0);
        let run = TextRun {
            text: String::from("Test"),
            range: (0, 4),
            script: String::from("Latin"),
            language: String::from("en"),
            direction: Direction::LeftToRight,
            font: None,
        };

        let shaped = backend.shape(&run, &font).unwrap();
        assert_eq!(shaped.glyphs.len(), 4);
    }
}
