// this_file: src/shaping.rs

//! Text shaping using HarfBuzz.
//!
//! This module shapes text into positioned glyphs, handling complex scripts,
//! ligatures, kerning, and other OpenType features.

use crate::error::{Error, Result};
use crate::fonts::FontInstance;
use harfbuzz_rs::{Direction, Face, Font as HbFont, GlyphBuffer, UnicodeBuffer};
use read_fonts::TableProvider;
use std::path::Path;

/// Shaped text with positioned glyphs.
#[derive(Debug, Clone)]
pub struct ShapedText {
    /// Positioned glyphs
    pub glyphs: Vec<ShapedGlyph>,
    /// Font size in points
    pub font_size: f32,
}

/// Single shaped glyph with position.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// Glyph ID in the font
    pub glyph_id: u32,
    /// Horizontal advance (in font units)
    pub x_advance: i32,
    /// Vertical advance (in font units, typically 0)
    pub y_advance: i32,
    /// Horizontal offset from cursor (in font units)
    pub x_offset: i32,
    /// Vertical offset from baseline (in font units)
    pub y_offset: i32,
}

/// Text shaper using HarfBuzz.
pub struct TextShaper;

impl TextShaper {
    /// Create a new text shaper.
    pub fn new() -> Self {
        Self
    }

    /// Shape text using the provided font instance.
    ///
    /// Returns positioned glyphs with advances and offsets.
    pub fn shape(
        &self,
        font_instance: &FontInstance,
        text: &str,
        font_size: f32,
        path: &Path,
    ) -> Result<ShapedText> {
        // Handle empty string
        if text.is_empty() {
            return Ok(ShapedText {
                glyphs: vec![],
                font_size,
            });
        }

        // Fast path for single character (common case for FontSimi)
        if text.chars().count() == 1 {
            return self.shape_single_char(font_instance, text, font_size, path);
        }

        // Full shaping path
        self.shape_harfbuzz(font_instance, text, font_size, path)
    }

    /// Fast path: shape single character without HarfBuzz overhead.
    fn shape_single_char(
        &self,
        font_instance: &FontInstance,
        text: &str,
        font_size: f32,
        _path: &Path,
    ) -> Result<ShapedText> {
        let ch = text.chars().next().unwrap();
        let font = font_instance.font_ref();

        // Map character to glyph ID
        let cmap = font
            .cmap()
            .map_err(|e| Error::Internal(format!("Failed to read cmap table: {}", e)))?;
        let glyph_id = cmap
            .map_codepoint(ch as u32)
            .ok_or_else(|| Error::Internal(format!("Character '{}' not found in font", ch)))?
            .to_u32();

        // Get advance width from hmtx table
        // TODO: Use instance coordinates for variable fonts
        if !font_instance.coordinates().is_empty() {
            log::warn!(
                "Single-character fast path does not support variable font coordinates yet: {:?}. Using static metrics.",
                font_instance.coordinates()
            );
        }
        let hmtx = font
            .hmtx()
            .map_err(|e| Error::Internal(format!("Failed to read hmtx table: {}", e)))?;
        let advance = hmtx.advance(glyph_id.into()).unwrap_or(0) as i32;

        Ok(ShapedText {
            glyphs: vec![ShapedGlyph {
                glyph_id,
                x_advance: advance,
                y_advance: 0,
                x_offset: 0,
                y_offset: 0,
            }],
            font_size,
        })
    }

    /// Full shaping using HarfBuzz.
    fn shape_harfbuzz(
        &self,
        font_instance: &FontInstance,
        text: &str,
        font_size: f32,
        path: &Path,
    ) -> Result<ShapedText> {
        // Get the raw font data from the FontInstance
        let font_data = font_instance.font_data();

        // Create HarfBuzz face from font data
        let face = Face::from_bytes(font_data, 0);
        let mut hb_font = HbFont::new(face);

        // Set font size (convert points to pixels, assuming 72 DPI)
        let ppem = font_size as u32;
        hb_font.set_ppem(ppem, ppem);

        // Apply variations if present
        if !font_instance.coordinates().is_empty() {
            let variations: Vec<harfbuzz_rs::Variation> = font_instance
                .coordinates()
                .iter()
                .filter_map(|(tag, value)| {
                    // Parse tag string (e.g. "wght") into 4 chars
                    let chars: Vec<char> = tag.chars().collect();
                    if chars.len() == 4 {
                        Some(harfbuzz_rs::Variation::new(
                            harfbuzz_rs::Tag::new(chars[0], chars[1], chars[2], chars[3]),
                            *value,
                        ))
                    } else {
                        None
                    }
                })
                .collect();

            hb_font.set_variations(&variations);
        }

        // Create buffer and add text (chain methods since they take ownership)
        let buffer = UnicodeBuffer::new()
            .add_str(text)
            .set_direction(Direction::Ltr)
            .guess_segment_properties();

        // Shape
        let glyph_buffer: GlyphBuffer = harfbuzz_rs::shape(&hb_font, buffer, &[]);

        // Extract glyph positions
        let glyph_infos = glyph_buffer.get_glyph_infos();
        let glyph_positions = glyph_buffer.get_glyph_positions();

        if glyph_infos.is_empty() {
            return Err(Error::ShapingFailed {
                text: text.to_string(),
                path: path.to_path_buf(),
                reason: "HarfBuzz returned zero glyphs".to_string(),
            });
        }

        let glyphs = glyph_infos
            .iter()
            .zip(glyph_positions.iter())
            .map(|(info, pos)| ShapedGlyph {
                glyph_id: info.codepoint,
                x_advance: pos.x_advance,
                y_advance: pos.y_advance,
                x_offset: pos.x_offset,
                y_offset: pos.y_offset,
            })
            .collect();

        Ok(ShapedText { glyphs, font_size })
    }
}

impl ShapedText {
    /// Calculate total advance width in font units.
    pub fn total_advance_width(&self) -> i32 {
        self.glyphs.iter().map(|g| g.x_advance).sum()
    }

    /// Calculate bounding box of all glyphs (in font units).
    pub fn bounding_box(&self) -> (i32, i32, i32, i32) {
        if self.glyphs.is_empty() {
            return (0, 0, 0, 0);
        }

        let mut min_x = i32::MAX;
        let mut min_y = i32::MAX;
        let mut max_x = i32::MIN;
        let mut max_y = i32::MIN;

        let mut cursor_x = 0i32;
        for glyph in &self.glyphs {
            let glyph_x = cursor_x + glyph.x_offset;
            let glyph_y = glyph.y_offset;

            min_x = min_x.min(glyph_x);
            min_y = min_y.min(glyph_y);
            max_x = max_x.max(glyph_x + glyph.x_advance);
            max_y = max_y.max(glyph_y + glyph.y_advance);

            cursor_x += glyph.x_advance;
        }

        (min_x, min_y, max_x - min_x, max_y - min_y)
    }
}

impl Default for TextShaper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shaped_text_empty() {
        let shaped = ShapedText {
            glyphs: vec![],
            font_size: 100.0,
        };
        assert_eq!(shaped.total_advance_width(), 0);
        assert_eq!(shaped.bounding_box(), (0, 0, 0, 0));
    }

    #[test]
    fn test_shaped_text_single_glyph() {
        let shaped = ShapedText {
            glyphs: vec![ShapedGlyph {
                glyph_id: 1,
                x_advance: 500,
                y_advance: 0,
                x_offset: 0,
                y_offset: 0,
            }],
            font_size: 100.0,
        };
        assert_eq!(shaped.total_advance_width(), 500);
    }
}
