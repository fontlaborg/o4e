// this_file: src/render.rs

//! Glyph rasterization and compositing using zeno.
//!
//! This module extracts glyph outlines from fonts and rasterizes them
//! into grayscale images with proper antialiasing.

use crate::error::{Error, Result};
use crate::fonts::FontInstance;
use crate::shaping::ShapedText;
use read_fonts::TableProvider;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::MetadataProvider;
use std::path::Path;
use zeno::{Command, Mask, Transform};

/// Glyph rasterizer using zeno.
pub struct GlyphRasterizer;

impl GlyphRasterizer {
    /// Create a new glyph rasterizer.
    pub fn new() -> Self {
        Self
    }

    /// Render shaped text to a grayscale image.
    ///
    /// Returns a vector of u8 pixels (grayscale, 0-255) in row-major order.
    pub fn render_text(
        &self,
        font_instance: &FontInstance,
        shaped: &ShapedText,
        width: u32,
        height: u32,
        tracking: f32,
        path: &Path,
    ) -> Result<Vec<u8>> {
        // Create blank canvas
        let mut canvas = vec![0u8; (width * height) as usize];

        if shaped.glyphs.is_empty() {
            return Ok(canvas);
        }

        let font = font_instance.font_ref();

        // TODO: Properly convert variation coordinates to normalized F2Dot14 values
        // For now, use default location (static font rendering only)
        if !font_instance.coordinates().is_empty() {
            log::warn!(
                "Variable font coordinates requested but not yet supported in rendering: {:?}. Using default coordinates.",
                font_instance.coordinates()
            );
        }
        let location_ref = LocationRef::default();

        // Calculate scale factor (font size to pixels)
        let head = font
            .head()
            .map_err(|e| Error::Internal(format!("Failed to read head table: {}", e)))?;
        let upem = head.units_per_em();
        let scale = shaped.font_size / upem as f32;

        // Position baseline at 75% height
        let baseline_y = height as f32 * 0.75;
        let mut cursor_x = 0.0f32;

        // Render each glyph
        for glyph in &shaped.glyphs {
            let glyph_id = glyph.glyph_id.into();

            // Extract outline
            let outline = font.outline_glyphs();
            let Some(glyph_outline) = outline.get(glyph_id) else {
                log::warn!("Glyph ID {} not found in font", glyph.glyph_id);
                cursor_x += (glyph.x_advance as f32 + tracking) * scale;
                continue;
            };

            // Build path
            let mut path_commands = Vec::new();
            let mut pen = ZenoPen::new(&mut path_commands);

            let draw_settings = DrawSettings::unhinted(Size::unscaled(), location_ref);
            if let Err(e) = glyph_outline.draw(draw_settings, &mut pen) {
                return Err(Error::RasterizationFailed {
                    glyph_id: glyph.glyph_id,
                    path: path.to_path_buf(),
                    reason: format!("Failed to draw outline: {}", e),
                });
            }

            // Calculate glyph position
            let glyph_x = cursor_x + (glyph.x_offset as f32 * scale);
            let glyph_y = baseline_y - (glyph.y_offset as f32 * scale);

            // Rasterize and composite
            self.composite_glyph(
                &mut canvas,
                &path_commands,
                glyph_x,
                glyph_y,
                scale,
                width,
                height,
            )?;

            // Advance cursor
            cursor_x += (glyph.x_advance as f32 + tracking) * scale;
        }

        Ok(canvas)
    }

    /// Composite a single glyph onto the canvas.
    fn composite_glyph(
        &self,
        canvas: &mut [u8],
        path: &[Command],
        x: f32,
        y: f32,
        scale: f32,
        width: u32,
        height: u32,
    ) -> Result<()> {
        // Create transform (scale + translate)
        let transform = Transform::scale(scale, scale).then_translate(x, y);

        // Rasterize to temporary mask
        let mut mask = Mask::new(path);
        mask.size(width, height).transform(Some(transform));

        let (alpha_data, placement) = mask.render();

        // Alpha blend onto canvas
        let top = placement.top.max(0) as u32;
        let left = placement.left.max(0) as u32;
        let bottom = (placement.top + placement.height as i32).min(height as i32) as u32;
        let right = (placement.left + placement.width as i32).min(width as i32) as u32;

        for py in top..bottom {
            for px in left..right {
                let canvas_idx = (py * width + px) as usize;
                let mask_y = (py as i32 - placement.top) as u32;
                let mask_x = (px as i32 - placement.left) as u32;
                let mask_idx = (mask_y * placement.width + mask_x) as usize;

                if mask_idx < alpha_data.len() {
                    let alpha = alpha_data[mask_idx];
                    let src = canvas[canvas_idx];

                    // Blend: dst + src * (1 - dst_alpha/255)
                    let blended =
                        src.saturating_add(((alpha as u16 * (255 - src) as u16) / 255) as u8);
                    canvas[canvas_idx] = blended;
                }
            }
        }

        Ok(())
    }

    /// Calculate actual bounding box of rendered content.
    pub fn calculate_bbox(pixels: &[u8], width: u32, height: u32) -> (u32, u32, u32, u32) {
        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0u32;
        let mut max_y = 0u32;

        for y in 0..height {
            for x in 0..width {
                let idx = (y * width + x) as usize;
                if pixels[idx] > 0 {
                    min_x = min_x.min(x);
                    min_y = min_y.min(y);
                    max_x = max_x.max(x);
                    max_y = max_y.max(y);
                }
            }
        }

        if min_x > max_x {
            // All pixels are zero (blank image)
            return (0, 0, 0, 0);
        }

        (min_x, min_y, max_x - min_x + 1, max_y - min_y + 1)
    }
}

impl Default for GlyphRasterizer {
    fn default() -> Self {
        Self::new()
    }
}

/// Adapter to convert skrifa OutlinePen to zeno command vector.
struct ZenoPen<'a> {
    commands: &'a mut Vec<Command>,
}

impl<'a> ZenoPen<'a> {
    fn new(commands: &'a mut Vec<Command>) -> Self {
        Self { commands }
    }
}

impl<'a> OutlinePen for ZenoPen<'a> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::MoveTo([x, -y].into())); // Flip Y for graphics coordinates
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(Command::LineTo([x, -y].into()));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.commands
            .push(Command::QuadTo([cx0, -cy0].into(), [x, -y].into()));
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.commands.push(Command::CurveTo(
            [cx0, -cy0].into(),
            [cx1, -cy1].into(),
            [x, -y].into(),
        ));
    }

    fn close(&mut self) {
        self.commands.push(Command::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_bbox_empty() {
        let pixels = vec![0u8; 100 * 50];
        let bbox = GlyphRasterizer::calculate_bbox(&pixels, 100, 50);
        assert_eq!(bbox, (0, 0, 0, 0));
    }

    #[test]
    fn test_calculate_bbox_single_pixel() {
        let mut pixels = vec![0u8; 100 * 50];
        pixels[25 * 100 + 50] = 255; // Pixel at (50, 25)

        let bbox = GlyphRasterizer::calculate_bbox(&pixels, 100, 50);
        assert_eq!(bbox, (50, 25, 1, 1));
    }

    #[test]
    fn test_calculate_bbox_rectangle() {
        let mut pixels = vec![0u8; 100 * 50];
        // Fill 10×5 rectangle starting at (20, 10)
        for y in 10..15 {
            for x in 20..30 {
                pixels[y * 100 + x] = 255;
            }
        }

        let bbox = GlyphRasterizer::calculate_bbox(&pixels, 100, 50);
        assert_eq!(bbox, (20, 10, 10, 5));
    }
}
