// this_file: backends/o4e-icu-hb/src/lib.rs

//! ICU+HarfBuzz backend for cross-platform text rendering.

use harfbuzz_rs::{Face as HbFace, Font as HbFont, Language, Owned, Tag, UnicodeBuffer};
use lru::LruCache;
use o4e_core::{
    types::Direction, Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions,
    RenderOutput, Result, SegmentOptions, ShapingResult, TextRun,
};
use o4e_unicode::TextSegmenter;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use ttf_parser::{Face as TtfFace, OutlineBuilder};

pub struct HarfBuzzBackend {
    cache: FontCache,
    hb_cache: RwLock<LruCache<String, Arc<Owned<HbFont<'static>>>>>,
    face_cache: RwLock<HashMap<String, Arc<Vec<u8>>>>,
    ttf_cache: RwLock<HashMap<String, Arc<TtfFace<'static>>>>,
    segmenter: TextSegmenter,
}

/// Outline builder for converting TrueType outlines to tiny-skia paths
struct SkiaOutlineBuilder {
    builder: PathBuilder,
    scale: f32,
}

impl OutlineBuilder for SkiaOutlineBuilder {
    fn move_to(&mut self, x: f32, y: f32) {
        self.builder.move_to(x * self.scale, -y * self.scale);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.builder.line_to(x * self.scale, -y * self.scale);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.builder.quad_to(
            x1 * self.scale,
            -y1 * self.scale,
            x * self.scale,
            -y * self.scale,
        );
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.builder.cubic_to(
            x1 * self.scale,
            -y1 * self.scale,
            x2 * self.scale,
            -y2 * self.scale,
            x * self.scale,
            -y * self.scale,
        );
    }

    fn close(&mut self) {
        self.builder.close();
    }
}

impl HarfBuzzBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
            hb_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            face_cache: RwLock::new(HashMap::new()),
            ttf_cache: RwLock::new(HashMap::new()),
            segmenter: TextSegmenter::new(),
        }
    }
    fn load_font_data(&self, font: &Font) -> Result<Arc<Vec<u8>>> {
        // Check cache first
        {
            let cache = self.face_cache.read();
            if let Some(data) = cache.get(&font.family) {
                return Ok(data.clone());
            }
        }

        // Try to load from file path
        let font_path = std::path::Path::new(&font.family);
        if font_path.exists() {
            let data = std::fs::read(font_path)
                .map_err(|e| O4eError::font_load(font_path.to_owned(), e))?;
            let data = Arc::new(data);

            let mut cache = self.face_cache.write();
            cache.insert(font.family.clone(), data.clone());

            return Ok(data);
        }

        // Try system fonts (simplified for now)
        let system_dirs = o4e_core::utils::system_font_dirs();
        for dir in system_dirs {
            let expanded = shellexpand::tilde(&dir);
            let dir_path = std::path::Path::new(expanded.as_ref());

            // Try with .ttf and .otf extensions
            for ext in &["ttf", "otf", "ttc"] {
                let font_file = dir_path.join(format!("{}.{}", font.family, ext));
                if font_file.exists() {
                    let data = std::fs::read(&font_file)
                        .map_err(|e| O4eError::font_load(font_file.clone(), e))?;
                    let data = Arc::new(data);

                    let mut cache = self.face_cache.write();
                    cache.insert(font.family.clone(), data.clone());

                    return Ok(data);
                }
            }
        }

        Err(O4eError::FontNotFound {
            name: font.family.clone(),
        })
    }

    fn get_or_create_ttf_face(&self, font: &Font) -> Result<Arc<TtfFace<'static>>> {
        let cache_key = font.family.clone();

        // Check cache
        {
            let cache = self.ttf_cache.read();
            if let Some(face) = cache.get(&cache_key) {
                return Ok(face.clone());
            }
        }

        // Load font data
        let font_data = self.load_font_data(font)?;

        // Create TtfFace
        // We need to leak the data to get 'static lifetime
        let leaked_data: &'static [u8] = Box::leak(font_data.to_vec().into_boxed_slice());

        let ttf_face =
            TtfFace::from_slice(leaked_data, 0).map_err(|_| O4eError::InvalidFontData)?;

        let ttf_face = Arc::new(ttf_face);

        // Cache it
        {
            let mut cache = self.ttf_cache.write();
            cache.insert(cache_key, ttf_face.clone());
        }

        Ok(ttf_face)
    }

    fn get_or_create_hb_font(&self, font: &Font) -> Result<Arc<Owned<HbFont<'static>>>> {
        let cache_key = format!("{}:{}", font.family, font.size as u32);

        // Check cache
        {
            let mut cache = self.hb_cache.write();
            if let Some(hb_font) = cache.get(&cache_key) {
                return Ok(hb_font.clone());
            }
        }

        // Load font data
        let font_data = self.load_font_data(font)?;

        // Create HarfBuzz font
        // We need to leak the data to get 'static lifetime for HarfBuzz
        let leaked_data: &'static [u8] = Box::leak(font_data.to_vec().into_boxed_slice());

        let hb_face = HbFace::from_bytes(leaked_data, 0);

        let mut hb_font = HbFont::new(hb_face);

        // Set font size in HarfBuzz units
        let _units_per_em = hb_font.face().upem() as f32;
        let scale = (font.size * 64.0) as i32; // Convert to 26.6 fixed point
        hb_font.set_scale(scale, scale);

        let hb_font = Arc::new(hb_font);

        // Cache it
        {
            let mut cache = self.hb_cache.write();
            cache.push(cache_key, hb_font.clone());
        }

        Ok(hb_font)
    }

    fn script_tag(script: &str) -> Tag {
        let lower = script.to_ascii_lowercase();
        match lower.as_str() {
            "latin" => Tag::new('L', 'a', 't', 'n'),
            "arabic" => Tag::new('A', 'r', 'a', 'b'),
            "hebrew" => Tag::new('H', 'e', 'b', 'r'),
            "cyrillic" => Tag::new('C', 'y', 'r', 'l'),
            "greek" => Tag::new('G', 'r', 'e', 'k'),
            "han" => Tag::new('H', 'a', 'n', 'i'),
            "hiragana" => Tag::new('H', 'i', 'r', 'a'),
            "katakana" => Tag::new('K', 'a', 'n', 'a'),
            "thai" => Tag::new('T', 'h', 'a', 'i'),
            "devanagari" => Tag::new('D', 'e', 'v', 'a'),
            _ => Tag::new('L', 'a', 't', 'n'),
        }
    }
}

impl Backend for HarfBuzzBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        self.segmenter.segment(text, options)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        let hb_font = self.get_or_create_hb_font(font)?;

        // Create script tag from script name
        let script_tag = Self::script_tag(&run.script);

        // Create HarfBuzz buffer
        let buffer = UnicodeBuffer::new()
            .add_str(&run.text)
            .set_direction(match run.direction {
                Direction::LeftToRight => harfbuzz_rs::Direction::Ltr,
                Direction::RightToLeft => harfbuzz_rs::Direction::Rtl,
                Direction::Auto => harfbuzz_rs::Direction::Ltr,
            })
            .set_script(script_tag)
            .set_language(Language::from_str(&run.language).unwrap_or_default());

        // Shape the text
        let output = harfbuzz_rs::shape(&hb_font, buffer, &[]);

        // Extract glyph information
        let mut glyphs = Vec::new();
        let mut x_pos = 0.0;
        let scale = font.size / hb_font.face().upem() as f32;

        let positions = output.get_glyph_positions();
        let infos = output.get_glyph_infos();

        for (info, pos) in infos.iter().zip(positions.iter()) {
            glyphs.push(Glyph {
                id: info.codepoint,
                cluster: info.cluster,
                x: x_pos + (pos.x_offset as f32 * scale),
                y: pos.y_offset as f32 * scale,
                advance: pos.x_advance as f32 * scale,
            });
            x_pos += pos.x_advance as f32 * scale;
        }

        let bbox = o4e_core::utils::calculate_bbox(&glyphs);

        Ok(ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance: x_pos,
            bbox,
            font: Some(font.clone()),
        })
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

        // Get the TrueType face for glyph rendering
        let ttf_face = self.get_or_create_ttf_face(font)?;

        // Calculate image dimensions
        let padding = options.padding as f32;
        let width = (shaped.bbox.width + padding * 2.0).ceil() as u32;
        let height = (shaped.bbox.height + padding * 2.0).ceil() as u32;

        // Create pixmap
        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| O4eError::render("Failed to create pixmap"))?;

        // Parse colors
        let (text_r, text_g, text_b, text_a) =
            o4e_core::utils::parse_color(&options.color).map_err(|e| O4eError::render(e))?;

        // Fill background if not transparent
        if options.background != "transparent" {
            let (bg_r, bg_g, bg_b, bg_a) = o4e_core::utils::parse_color(&options.background)
                .map_err(|e| O4eError::render(e))?;
            pixmap.fill(Color::from_rgba8(bg_r, bg_g, bg_b, bg_a));
        }

        // Create paint for text rendering
        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(text_r, text_g, text_b, text_a));
        paint.anti_alias = options.antialias != o4e_core::types::AntialiasMode::None;

        // Calculate scale factor
        let units_per_em = ttf_face.units_per_em();
        let scale = font.size / units_per_em as f32;

        // Calculate baseline position
        let ascender = ttf_face.ascender() as f32 * scale;
        let baseline_y = padding + ascender;

        // Render each glyph
        for glyph in &shaped.glyphs {
            let glyph_id = ttf_parser::GlyphId(glyph.id as u16);

            // Build glyph outline
            let mut builder = SkiaOutlineBuilder {
                builder: PathBuilder::new(),
                scale,
            };

            if ttf_face.outline_glyph(glyph_id, &mut builder).is_some() {
                if let Some(path) = builder.builder.finish() {
                    // Apply glyph transform
                    let transform = Transform::from_translate(glyph.x + padding, baseline_y);

                    pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);
                }
            }
        }

        // Convert to requested format
        match options.format {
            o4e_core::types::RenderFormat::Raw => {
                let bitmap = Bitmap {
                    width,
                    height,
                    data: pixmap.data().to_vec(),
                };
                Ok(RenderOutput::Bitmap(bitmap))
            }
            o4e_core::types::RenderFormat::Png => {
                // Encode as PNG
                let mut png_data = Vec::new();
                {
                    let mut encoder = png::Encoder::new(&mut png_data, width, height);
                    encoder.set_color(png::ColorType::Rgba);
                    encoder.set_depth(png::BitDepth::Eight);
                    let mut writer = encoder
                        .write_header()
                        .map_err(|e| O4eError::render(format!("PNG encoding error: {}", e)))?;
                    writer
                        .write_image_data(pixmap.data())
                        .map_err(|e| O4eError::render(format!("PNG write error: {}", e)))?;
                }
                Ok(RenderOutput::Png(png_data))
            }
            o4e_core::types::RenderFormat::Svg => {
                // SVG rendering using o4e-render
                let svg_options = o4e_core::types::SvgOptions::default();
                let renderer = o4e_render::SvgRenderer::new(&svg_options);
                let svg = renderer.render(&shaped, &svg_options);
                Ok(RenderOutput::Svg(svg))
            }
        }
    }

    fn name(&self) -> &str {
        "HarfBuzz+ICU"
    }

    fn clear_cache(&self) {
        self.cache.clear();
        self.hb_cache.write().clear();
        self.face_cache.write().clear();
        self.ttf_cache.write().clear();
    }
}

impl Default for HarfBuzzBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture_font_path(name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .join("../../testdata/fonts")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn fixture_font(name: &str) -> Font {
        Font::new(fixture_font_path(name), 48.0)
    }

    #[test]
    fn test_backend_creation() {
        let backend = HarfBuzzBackend::new();
        assert_eq!(backend.name(), "HarfBuzz+ICU");
    }

    #[test]
    fn test_simple_segmentation() {
        let backend = HarfBuzzBackend::new();
        let options = SegmentOptions::default();

        let runs = backend.segment("Hello World", &options).unwrap();
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].text, "Hello World");
        assert_eq!(runs[0].script, "Latin");
        assert_eq!(runs[0].direction, Direction::LeftToRight);
    }

    #[test]
    fn test_script_itemization_and_bidi() {
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;

        let runs = backend.segment("Hello مرحبا", &options).unwrap();
        assert!(runs.len() >= 2);
        assert_eq!(runs[0].script, "Latin");
        assert_eq!(runs[0].direction, Direction::LeftToRight);
        assert_eq!(runs.last().unwrap().script, "Arabic");
        assert_eq!(runs.last().unwrap().direction, Direction::RightToLeft);
    }

    #[test]
    fn test_line_breaks_split_runs() {
        let backend = HarfBuzzBackend::new();
        let options = SegmentOptions::default();
        let runs = backend.segment("Line1\nLine2", &options).unwrap();
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0].text.trim_end_matches('\n'), "Line1");
        assert_eq!(runs[1].text, "Line2");
    }

    #[test]
    fn test_word_boundaries_when_font_fallback_enabled() {
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.font_fallback = true;
        let runs = backend.segment("Word One", &options).unwrap();
        assert!(runs.len() >= 2);
    }

    #[test]
    fn test_shape_arabic_text_produces_contextual_forms() {
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("ar".to_string());
        let text = "مرحبا بالعالم";

        let runs = backend.segment(text, &options).unwrap();
        assert_eq!(runs.len(), 1, "Arabic text should stay in a single run");
        let run = &runs[0];
        assert_eq!(
            run.direction,
            Direction::RightToLeft,
            "Arabic run must resolve to RTL"
        );

        let font = fixture_font("NotoNaskhArabic-Regular.ttf");
        let shaped = backend.shape(run, &font).expect("Arabic shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        let expected_ids = vec![486, 452, 4, 309, 452, 4, 38, 1374, 4, 37, 140, 212, 488];
        assert_eq!(
            glyph_ids, expected_ids,
            "Arabic contextual glyph ids regressed"
        );

        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert!(
            clusters.windows(2).all(|pair| pair[0] > pair[1]),
            "Arabic clusters should decrease for RTL text: {clusters:?}"
        );
        assert_eq!(
            clusters.last(),
            Some(&0),
            "Arabic clusters must end at byte offset 0"
        );
        assert!(
            shaped.advance > 0.0 && shaped.bbox.width > 0.0,
            "Arabic shaping should produce measurable geometry"
        );
    }

    #[test]
    fn test_shape_devanagari_text_reorders_marks() {
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("hi".to_string());
        let text = "कक्षा में";

        let runs = backend.segment(text, &options).unwrap();
        assert_eq!(runs.len(), 1, "Devanagari text should be a single run");
        let run = &runs[0];
        assert_eq!(run.script, "Devanagari");
        assert_eq!(run.direction, Direction::LeftToRight);

        let font = fixture_font("NotoSansDevanagari-Regular.ttf");
        let shaped = backend
            .shape(run, &font)
            .expect("Devanagari shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        let expected_ids = vec![25, 179, 66, 3, 50, 449];
        assert_eq!(glyph_ids, expected_ids, "Devanagari glyph ids changed");

        let clusters: Vec<u32> = shaped.glyphs.iter().map(|g| g.cluster).collect();
        assert!(
            clusters.windows(2).all(|pair| pair[0] <= pair[1]),
            "LTR clusters must be non-decreasing: {clusters:?}"
        );

        assert_eq!(
            shaped.glyphs[1].cluster, shaped.glyphs[2].cluster,
            "AA matra must attach to the conjunct cluster"
        );
        assert!(
            shaped.glyphs.iter().any(|g| g.advance == 0.0),
            "At least one mark should have zero advance after reordering"
        );
    }
}
