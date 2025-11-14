// this_file: backends/o4e-mac/src/lib.rs

//! CoreText backend for macOS text rendering.

#![cfg(target_os = "macos")]

use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, TCFType},
    string::CFString,
};
use core_graphics::{
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    geometry::{CGPoint, CGRect, CGSize},
};
use core_text::{
    font::{new_from_name, CTFont},
    line::CTLine,
    string_attributes::kCTFontAttributeName,
};
use lru::LruCache;
use o4e_core::{
    types::RenderFormat, Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions,
    RenderOutput, Result, SegmentOptions, ShapingResult, TextRun,
};
use o4e_unicode::TextSegmenter;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;

pub struct CoreTextBackend {
    cache: FontCache,
    ct_font_cache: RwLock<LruCache<String, Arc<CTFont>>>,
    shape_cache: RwLock<LruCache<String, Arc<ShapingResult>>>,
    segmenter: TextSegmenter,
}

impl CoreTextBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
            ct_font_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            shape_cache: RwLock::new(LruCache::new(NonZeroUsize::new(256).unwrap())),
            segmenter: TextSegmenter::new(),
        }
    }

    fn get_or_create_ct_font(&self, font: &Font) -> Result<Arc<CTFont>> {
        let cache_key = format!("{}:{}", font.family, font.size as u32);

        // Check cache
        {
            let mut cache = self.ct_font_cache.write();
            if let Some(ct_font) = cache.get(&cache_key) {
                return Ok(ct_font.clone());
            }
        }

        // Create new CTFont
        let ct_font =
            new_from_name(&font.family, font.size as f64).map_err(|_| O4eError::FontNotFound {
                name: font.family.clone(),
            })?;

        let ct_font = Arc::new(ct_font);

        // Cache it
        {
            let mut cache = self.ct_font_cache.write();
            cache.push(cache_key, ct_font.clone());
        }

        Ok(ct_font)
    }

    fn create_attributed_string(
        &self,
        text: &str,
        font: &Font,
    ) -> Result<CFMutableAttributedString> {
        let ct_font = self.get_or_create_ct_font(font)?;

        let cf_string = CFString::new(text);

        // Create mutable attributed string
        let mut attributed_string = CFMutableAttributedString::new();
        attributed_string.replace_str(&cf_string, CFRange::init(0, 0));

        // Apply font attribute
        let length = attributed_string.char_len();
        attributed_string.set_attribute(
            CFRange::init(0, length),
            unsafe { kCTFontAttributeName },
            &*ct_font,
        );

        // Return the mutable attributed string
        Ok(attributed_string)
    }
}

impl Backend for CoreTextBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        self.segmenter.segment(text, options)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // Check cache
        let cache_key = format!("{}:{}:{}", run.text, font.family, font.size as u32);
        {
            let mut cache = self.shape_cache.write();
            if let Some(result) = cache.get(&cache_key) {
                return Ok((**result).clone());
            }
        }

        // Create attributed string and CTLine
        let attributed_string = self.create_attributed_string(&run.text, font)?;
        let line = CTLine::new_with_attributed_string(attributed_string.as_concrete_TypeRef());

        // For simplicity, we'll use a basic approximation rather than extracting individual glyphs
        // CoreText's CTLine gives us the overall bounds and positions
        let bounds = line.get_typographic_bounds();
        let width = bounds.width as f32;

        // Create glyphs based on character positions
        // This is a simplified approach - in production, we'd properly extract glyphs
        let mut glyphs = Vec::new();
        let mut x_offset = 0.0;

        // For each character, create a basic glyph entry
        for (idx, ch) in run.text.char_indices() {
            // Approximate advance based on character width
            let advance = width / run.text.chars().count() as f32;

            glyphs.push(Glyph {
                id: ch as u32, // Using character code as glyph ID (simplified)
                cluster: idx as u32,
                x: x_offset,
                y: 0.0,
                advance,
            });
            x_offset += advance;
        }

        let bbox = o4e_core::utils::calculate_bbox(&glyphs);

        let result = ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance: width,
            bbox,
            font: Some(font.clone()),
        };

        let result = Arc::new(result);

        // Cache the result
        {
            let mut cache = self.shape_cache.write();
            cache.push(cache_key, result.clone());
        }

        Ok((*result).clone())
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // Check if we have glyphs to render
        if shaped.glyphs.is_empty() {
            return Ok(RenderOutput::Bitmap(Bitmap {
                width: 1,
                height: 1,
                data: vec![0, 0, 0, 0],
            }));
        }

        // Get the font from ShapingResult
        let font = shaped
            .font
            .as_ref()
            .ok_or_else(|| O4eError::render("Font information missing from shaped result"))?;

        // Calculate image dimensions
        let padding = options.padding as f32;
        let width = (shaped.bbox.width + padding * 2.0).ceil() as usize;
        let height = (shaped.bbox.height + padding * 2.0).ceil() as usize;

        // Create CGContext for rendering
        let bytes_per_row = width * 4; // RGBA
        let mut buffer = vec![0u8; height * bytes_per_row];

        let color_space = CGColorSpace::create_device_rgb();
        let context = CGContext::create_bitmap_context(
            Some(buffer.as_mut_ptr() as *mut _),
            width,
            height,
            8,
            bytes_per_row,
            &color_space,
            core_graphics::base::kCGImageAlphaPremultipliedLast,
        );

        // Parse colors
        let (text_r, text_g, text_b, text_a) =
            o4e_core::utils::parse_color(&options.color).map_err(|e| O4eError::render(e))?;

        // Fill background if not transparent
        if options.background != "transparent" {
            let (bg_r, bg_g, bg_b, bg_a) = o4e_core::utils::parse_color(&options.background)
                .map_err(|e| O4eError::render(e))?;
            context.set_rgb_fill_color(
                bg_r as f64 / 255.0,
                bg_g as f64 / 255.0,
                bg_b as f64 / 255.0,
                bg_a as f64 / 255.0,
            );
            context.fill_rect(CGRect::new(
                &CGPoint::new(0.0, 0.0),
                &CGSize::new(width as f64, height as f64),
            ));
        }

        // Set text color
        context.set_rgb_fill_color(
            text_r as f64 / 255.0,
            text_g as f64 / 255.0,
            text_b as f64 / 255.0,
            text_a as f64 / 255.0,
        );

        // Get CTFont
        let ct_font = self.get_or_create_ct_font(font)?;

        // Flip coordinate system (CoreGraphics uses bottom-left origin)
        context.translate(0.0, height as f64);
        context.scale(1.0, -1.0);

        // Calculate baseline position
        let baseline_y = padding as f64 + ct_font.ascent();

        // Recreate text via CoreText using the shaped run text
        let text_to_render = if shaped.text.trim().is_empty() {
            " "
        } else {
            shaped.text.as_str()
        };

        // Create attributed string and line for rendering
        let attributed_string = self.create_attributed_string(text_to_render, font)?;
        let line = CTLine::new_with_attributed_string(attributed_string.as_concrete_TypeRef());

        // Draw the text
        context.save();
        context.translate(padding as f64, baseline_y);
        context.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
        line.draw(&context);
        context.restore();

        // Convert to requested format
        match options.format {
            RenderFormat::Raw => {
                let bitmap = Bitmap {
                    width: width as u32,
                    height: height as u32,
                    data: buffer,
                };
                Ok(RenderOutput::Bitmap(bitmap))
            }
            RenderFormat::Png => {
                // Encode as PNG
                let mut png_data = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut png_data, width as u32, height as u32);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder
                        .write_header()
                        .map_err(|e| O4eError::render(format!("PNG encoding error: {}", e)))?;
                    writer
                        .write_image_data(&buffer)
                        .map_err(|e| O4eError::render(format!("PNG write error: {}", e)))?;
                }
                Ok(RenderOutput::Png(png_data))
            }
            RenderFormat::Svg => {
                // SVG rendering using o4e-render
                let svg_options = o4e_core::types::SvgOptions::default();
                let renderer = o4e_render::SvgRenderer::new(&svg_options);
                let svg = renderer.render(&shaped, &svg_options);
                Ok(RenderOutput::Svg(svg))
            }
        }
    }

    fn name(&self) -> &str {
        "CoreText"
    }

    fn clear_cache(&self) {
        self.cache.clear();
        self.ct_font_cache.write().clear();
        self.shape_cache.write().clear();
    }
}

impl Default for CoreTextBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use o4e_core::types::Direction;

    fn assert_script_rendered(text: &str, font_name: &str) {
        let backend = CoreTextBackend::new();
        let font = Font::new(font_name, 42.0);

        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!(
                "Skipping CoreText script test because font '{}' is unavailable on this system",
                font_name
            );
            return;
        }

        let mut segment_options = SegmentOptions::default();
        segment_options.script_itemize = true;
        segment_options.bidi_resolve = true;

        let runs = backend.segment(text, &segment_options).unwrap();
        assert!(
            !runs.is_empty(),
            "CoreText should produce at least one run for '{}':{}",
            font_name,
            text
        );

        let render_options = RenderOptions::default();
        let mut reconstructed = String::new();

        for run in runs {
            let shaped = backend.shape(&run, &font).unwrap();
            assert_eq!(shaped.text, run.text);
            assert!(
                !shaped.glyphs.is_empty(),
                "Shaping should yield glyphs for '{}' using font '{}'",
                text,
                font_name
            );
            reconstructed.push_str(&shaped.text);

            match backend.render(&shaped, &render_options).unwrap() {
                RenderOutput::Bitmap(bitmap) => {
                    assert!(bitmap.width > 0);
                    assert!(bitmap.height > 0);
                    assert!(!bitmap.data.is_empty());
                }
                other => panic!(
                    "CoreText raw rendering should return a bitmap, got {:?}",
                    other
                ),
            }
        }

        assert_eq!(reconstructed, text);
    }

    #[test]
    fn test_backend_creation() {
        let backend = CoreTextBackend::new();
        assert_eq!(backend.name(), "CoreText");
    }

    #[test]
    fn test_simple_segmentation() {
        let backend = CoreTextBackend::new();
        let options = SegmentOptions::default();

        let runs = backend.segment("Hello World", &options).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello World");
    }

    #[test]
    fn test_segment_latin_text_reports_script_and_direction() {
        let backend = CoreTextBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;

        let runs = backend.segment("Hello World", &options).unwrap();
        assert_eq!(runs.len(), 1, "Latin text should remain a single run");
        let run = &runs[0];
        assert_eq!(run.script, "Latin");
        assert_eq!(run.direction, Direction::LeftToRight);
    }

    #[test]
    fn test_segment_arabic_text_detects_rtl_run() {
        let backend = CoreTextBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;

        let runs = backend.segment("مرحبا بالعالم", &options).unwrap();
        assert!(
            !runs.is_empty(),
            "Arabic text should yield at least one run"
        );
        let arabic_run = runs
            .iter()
            .find(|run| run.script == "Arabic")
            .expect("Arabic run not detected");
        assert_eq!(arabic_run.direction, Direction::RightToLeft);
    }

    #[test]
    fn test_segment_cjk_text_detects_han_script() {
        let backend = CoreTextBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;

        let runs = backend.segment("漢字テスト", &options).unwrap();
        assert!(
            runs.iter().any(|run| run.script == "Han"),
            "Expected at least one Han-script run"
        );
    }

    #[test]
    fn test_coretext_render_when_latin_text_provided() {
        assert_script_rendered("Hello CoreText", "Helvetica");
    }

    #[test]
    fn test_coretext_render_when_arabic_text_provided() {
        assert_script_rendered("مرحبا بالعالم", "Geeza Pro");
    }

    #[test]
    fn test_coretext_render_when_cjk_text_provided() {
        assert_script_rendered("你好世界", "PingFang SC");
    }
}
