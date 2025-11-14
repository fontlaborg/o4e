// this_file: backends/o4e-icu-hb/src/lib.rs

//! ICU+HarfBuzz backend for cross-platform text rendering.

use harfbuzz_rs::{Face as HbFace, Font as HbFont, Language, Owned, Tag, UnicodeBuffer};
use kurbo::{BezPath, PathEl};
use lru::LruCache;
use o4e_core::{
    cache::{FontKey, GlyphKey, RenderedGlyph},
    types::{Direction, FontSource},
    utils::{calculate_bbox, quantize_size},
    Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions, RenderOutput, Result,
    SegmentOptions, ShapingResult, TextRun,
};
use o4e_fontdb::{script_fallbacks, FontDatabase, FontHandle};
use o4e_render::outlines::glyph_bez_path as recorded_glyph_path;
use o4e_unicode::TextSegmenter;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::str::FromStr;
use std::sync::Arc;
use tiny_skia::{
    Color, FillRule, Paint, Path as SkiaPath, PathBuilder, Pixmap, PixmapPaint, PixmapRef,
    Transform,
};
use ttf_parser::{Face as TtfFace, GlyphId};

pub struct HarfBuzzBackend {
    cache: FontCache,
    hb_cache: RwLock<LruCache<String, Arc<HbFontEntry>>>,
    ttf_cache: RwLock<HashMap<String, Arc<TtfFaceEntry>>>,
    font_data_cache: RwLock<HashMap<String, Arc<FontDataEntry>>>,
    font_db: &'static FontDatabase,
    segmenter: TextSegmenter,
}

#[derive(Clone, Debug)]
struct FontDataEntry {
    key: String,
    path: Option<PathBuf>,
    bytes: Arc<[u8]>,
    face_index: u32,
}

impl FontDataEntry {
    fn from_handle(handle: Arc<FontHandle>) -> Self {
        Self {
            key: handle.key.clone(),
            path: handle.path.clone(),
            bytes: handle.bytes.clone(),
            face_index: handle.face_index,
        }
    }

    fn as_static_slice(&self) -> &'static [u8] {
        // Safety: the underlying Arc<[u8]> remains alive as long as this entry does.
        unsafe { std::mem::transmute::<&[u8], &'static [u8]>(self.bytes.as_ref()) }
    }

    fn key(&self) -> String {
        self.key.clone()
    }

    fn font_key(&self) -> FontKey {
        FontKey {
            path: PathBuf::from(&self.key),
            face_index: self.face_index,
        }
    }
}

#[derive(Debug)]
struct HbFontEntry {
    data: Arc<FontDataEntry>,
    font: Owned<HbFont<'static>>,
}

impl HbFontEntry {
    fn new(data: Arc<FontDataEntry>, size: f32) -> Result<Self> {
        let hb_face = HbFace::new(data.bytes.clone(), data.face_index);
        let mut hb_font = HbFont::new(hb_face);

        let scale = (size * 64.0).max(1.0) as i32;
        hb_font.set_scale(scale, scale);

        Ok(Self {
            data,
            font: hb_font,
        })
    }

    fn font(&self) -> &HbFont<'static> {
        &self.font
    }
}

#[derive(Debug)]
struct TtfFaceEntry {
    data: Arc<FontDataEntry>,
    face: TtfFace<'static>,
}

impl TtfFaceEntry {
    fn new(data: Arc<FontDataEntry>) -> Result<Self> {
        let face = TtfFace::parse(data.as_static_slice(), data.face_index)
            .map_err(|_| O4eError::InvalidFontData)?;
        Ok(Self { data, face })
    }

    fn face(&self) -> &TtfFace<'static> {
        &self.face
    }

    fn font_key(&self) -> FontKey {
        self.data.font_key()
    }
}

impl HarfBuzzBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
            hb_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            ttf_cache: RwLock::new(HashMap::new()),
            font_data_cache: RwLock::new(HashMap::new()),
            font_db: FontDatabase::global(),
            segmenter: TextSegmenter::new(),
        }
    }

    fn load_font_data(&self, font: &Font) -> Result<Arc<FontDataEntry>> {
        let handle = self.font_db.resolve(font)?;
        let key = handle.key.clone();
        if let Some(entry) = self.font_data_cache.read().get(&key) {
            return Ok(entry.clone());
        }

        let entry = Arc::new(FontDataEntry::from_handle(handle));
        self.font_data_cache.write().insert(key, entry.clone());
        Ok(entry)
    }
    fn get_or_create_ttf_face(&self, font: &Font) -> Result<Arc<TtfFaceEntry>> {
        let font_data = self.load_font_data(font)?;
        let cache_key = font_data.key();

        if let Some(entry) = self.ttf_cache.read().get(&cache_key) {
            return Ok(entry.clone());
        }

        let entry = Arc::new(TtfFaceEntry::new(font_data)?);
        self.ttf_cache.write().insert(cache_key, entry.clone());
        Ok(entry)
    }

    fn get_or_create_hb_font(&self, font: &Font) -> Result<Arc<HbFontEntry>> {
        let font_data = self.load_font_data(font)?;
        let cache_key = format!("{}:{}", font_data.key(), quantize_size(font.size));

        {
            let mut cache = self.hb_cache.write();
            if let Some(entry) = cache.get(&cache_key) {
                return Ok(entry.clone());
            }
        }

        let entry = Arc::new(HbFontEntry::new(font_data, font.size)?);
        {
            let mut cache = self.hb_cache.write();
            cache.push(cache_key, entry.clone());
        }
        Ok(entry)
    }

    fn resolve_run_font(&self, run: &TextRun, requested: &Font) -> Font {
        if let Some(run_font) = run.font.as_ref() {
            if self.font_supports_run(run_font, run) {
                return run_font.clone();
            }
        }

        if self.font_supports_run(requested, run) {
            return requested.clone();
        }

        for candidate in script_fallbacks(&run.script) {
            let mut fallback = requested.clone();
            fallback.family = candidate.to_string();
            fallback.source = FontSource::Family(candidate.to_string());
            if self.font_supports_run(&fallback, run) {
                return fallback;
            }
        }

        log::warn!(
            "No fallback font found for script '{}' using '{}'; falling back to specified font",
            run.script,
            requested.family
        );
        requested.clone()
    }

    fn font_supports_run(&self, font: &Font, run: &TextRun) -> bool {
        match self.get_or_create_ttf_face(font) {
            Ok(entry) => run
                .text
                .chars()
                .all(|ch| entry.face().glyph_index(ch).is_some()),
            Err(_) => false,
        }
    }

    fn rasterize_glyph(
        &self,
        ttf_face: &TtfFace<'static>,
        glyph: &Glyph,
        scale: f32,
        antialias: bool,
    ) -> Option<RenderedGlyph> {
        let path = match glyph_path(ttf_face, glyph, scale) {
            Some(path) => path,
            None => return Some(blank_rendered_glyph()),
        };

        let bounds = path.bounds();
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return Some(blank_rendered_glyph());
        }

        let width = bounds.width().ceil().max(1.0) as u32;
        let height = bounds.height().ceil().max(1.0) as u32;
        let mut mask_pixmap = Pixmap::new(width, height)?;

        let mut paint = Paint::default();
        paint.set_color(Color::from_rgba8(255, 255, 255, 255));
        paint.anti_alias = antialias;

        let transform = Transform::from_translate(-bounds.left(), -bounds.top());
        mask_pixmap.fill_path(&path, &paint, FillRule::Winding, transform, None);

        let mut mask = Vec::with_capacity((width * height) as usize);
        for pixel in mask_pixmap.data().chunks_exact(4) {
            mask.push(pixel[3]);
        }

        Some(RenderedGlyph {
            bitmap: mask,
            width,
            height,
            left: bounds.left(),
            top: bounds.top(),
        })
    }

    fn draw_cached_glyph(
        &self,
        target: &mut Pixmap,
        glyph: &Glyph,
        cached: &RenderedGlyph,
        baseline_y: f32,
        padding: f32,
        scratch: &mut Vec<u8>,
        base_r: u16,
        base_g: u16,
        base_b: u16,
        text_alpha: u8,
    ) {
        if cached.width == 0 || cached.height == 0 {
            return;
        }

        let pixels = (cached.width * cached.height) as usize;
        let required = pixels * 4;
        scratch.clear();
        scratch.resize(required, 0);

        let alpha_component = u16::from(text_alpha);
        for (idx, coverage) in cached.bitmap.iter().enumerate() {
            let cov = u16::from(*coverage);
            let offset = idx * 4;
            scratch[offset] = ((base_r * cov + 127) / 255) as u8;
            scratch[offset + 1] = ((base_g * cov + 127) / 255) as u8;
            scratch[offset + 2] = ((base_b * cov + 127) / 255) as u8;
            scratch[offset + 3] = ((alpha_component * cov + 127) / 255) as u8;
        }

        let Some(pixmap_ref) =
            PixmapRef::from_bytes(&scratch[..required], cached.width, cached.height)
        else {
            return;
        };

        let dest_x = glyph.x + padding + cached.left;
        let dest_y = baseline_y + cached.top;
        let base_x = dest_x.floor() as i32;
        let base_y = dest_y.floor() as i32;
        let frac_x = dest_x - base_x as f32;
        let frac_y = dest_y - base_y as f32;

        let paint = PixmapPaint::default();
        target.draw_pixmap(
            base_x,
            base_y,
            pixmap_ref,
            &paint,
            Transform::from_translate(frac_x, frac_y),
            None,
        );
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
        let resolved_font = self.resolve_run_font(run, font);
        let hb_entry = self.get_or_create_hb_font(&resolved_font)?;
        let hb_font = hb_entry.font();

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
        let output = harfbuzz_rs::shape(hb_font, buffer, &[]);

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

        let bbox = calculate_bbox(&glyphs);

        Ok(ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance: x_pos,
            bbox,
            font: Some(resolved_font),
            direction: run.direction,
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
        let face_entry = self.get_or_create_ttf_face(font)?;
        let ttf_face = face_entry.face();

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

        // Calculate scale factor
        let units_per_em = ttf_face.units_per_em();
        let scale = font.size / units_per_em as f32;

        // Calculate baseline position
        let ascender = ttf_face.ascender() as f32 * scale;
        let baseline_y = padding + ascender;

        let font_key = face_entry.font_key();
        let glyph_size = quantize_size(font.size);
        let mut scratch_rgba = Vec::new();
        let base_r = (u16::from(text_r) * u16::from(text_a) + 127) / 255;
        let base_g = (u16::from(text_g) * u16::from(text_a) + 127) / 255;
        let base_b = (u16::from(text_b) * u16::from(text_a) + 127) / 255;

        // Render each glyph using the shared glyph cache
        for glyph in &shaped.glyphs {
            let glyph_key = GlyphKey {
                font_key: font_key.clone(),
                glyph_id: glyph.id,
                size: glyph_size,
            };

            let cached = if let Some(entry) = self.cache.get_glyph(&glyph_key) {
                entry
            } else {
                match self.rasterize_glyph(
                    ttf_face,
                    glyph,
                    scale,
                    options.antialias != o4e_core::types::AntialiasMode::None,
                ) {
                    Some(rendered) => self.cache.cache_glyph(glyph_key, rendered),
                    None => continue,
                }
            };

            self.draw_cached_glyph(
                &mut pixmap,
                glyph,
                cached.as_ref(),
                baseline_y,
                padding,
                &mut scratch_rgba,
                base_r,
                base_g,
                base_b,
                text_a,
            );
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
        self.font_data_cache.write().clear();
        self.ttf_cache.write().clear();
    }
}

impl Default for HarfBuzzBackend {
    fn default() -> Self {
        Self::new()
    }
}

fn glyph_path(ttf_face: &TtfFace<'static>, glyph: &Glyph, scale: f32) -> Option<SkiaPath> {
    let gid = u16::try_from(glyph.id).ok()?;
    let outline = recorded_glyph_path(ttf_face, GlyphId(gid), scale)?;
    bez_path_to_skia(&outline)
}

fn bez_path_to_skia(path: &BezPath) -> Option<SkiaPath> {
    if path.elements().is_empty() {
        return None;
    }

    let mut builder = PathBuilder::new();
    for element in path.elements() {
        match *element {
            PathEl::MoveTo(p) => builder.move_to(p.x as f32, p.y as f32),
            PathEl::LineTo(p) => builder.line_to(p.x as f32, p.y as f32),
            PathEl::QuadTo(ctrl, end) => {
                builder.quad_to(ctrl.x as f32, ctrl.y as f32, end.x as f32, end.y as f32)
            }
            PathEl::CurveTo(c1, c2, end) => builder.cubic_to(
                c1.x as f32,
                c1.y as f32,
                c2.x as f32,
                c2.y as f32,
                end.x as f32,
                end.y as f32,
            ),
            PathEl::ClosePath => builder.close(),
        }
    }
    builder.finish()
}

fn blank_rendered_glyph() -> RenderedGlyph {
    RenderedGlyph {
        bitmap: Vec::new(),
        width: 0,
        height: 0,
        left: 0.0,
        top: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use std::collections::HashSet;
    use std::{fs, path::PathBuf, sync::Once};

    #[derive(Deserialize)]
    struct ShapeFixture {
        text: String,
        glyph_ids: Vec<u32>,
        font: String,
    }

    fn fixture_font_path(name: &str) -> String {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        PathBuf::from(manifest_dir)
            .join("../../testdata/fonts")
            .join(name)
            .to_string_lossy()
            .into_owned()
    }

    fn fixture_font(name: &str) -> Font {
        Font::from_path(fixture_font_path(name), 48.0)
    }

    fn ensure_test_fonts() {
        static INSTALL: Once = Once::new();
        INSTALL.call_once(|| {
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fonts");
            let existing = std::env::var_os("O4E_FONT_DIRS");
            let mut paths: Vec<PathBuf> = existing
                .map(|value| std::env::split_paths(&value).collect())
                .unwrap_or_default();
            if !paths.iter().any(|p| p == &dir) {
                paths.push(dir.clone());
            }
            let joined = std::env::join_paths(paths).expect("join font dirs");
            std::env::set_var("O4E_FONT_DIRS", joined);
        });
    }

    fn load_fixture(name: &str) -> ShapeFixture {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join(format!("../../testdata/expected/harfbuzz/{name}.json"));
        let data = fs::read_to_string(&path).expect("fixture readable");
        serde_json::from_str(&data).expect("fixture valid")
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
        ensure_test_fonts();
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("ar".to_string());
        let fixture = load_fixture("arabic_glyphs");

        let runs = backend.segment(&fixture.text, &options).unwrap();
        assert_eq!(runs.len(), 1, "Arabic text should stay in a single run");
        let run = &runs[0];
        assert_eq!(
            run.direction,
            Direction::RightToLeft,
            "Arabic run must resolve to RTL"
        );

        let font = fixture_font(&fixture.font);
        let shaped = backend.shape(run, &font).expect("Arabic shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        assert_eq!(glyph_ids, fixture.glyph_ids, "Arabic glyph ids regressed");

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
        ensure_test_fonts();
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("hi".to_string());
        let fixture = load_fixture("devanagari_glyphs");

        let runs = backend.segment(&fixture.text, &options).unwrap();
        assert_eq!(runs.len(), 1, "Devanagari text should be a single run");
        let run = &runs[0];
        assert_eq!(run.script, "Devanagari");
        assert_eq!(run.direction, Direction::LeftToRight);

        let font = fixture_font(&fixture.font);
        let shaped = backend
            .shape(run, &font)
            .expect("Devanagari shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        assert_eq!(glyph_ids, fixture.glyph_ids, "Devanagari glyph ids changed");

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

    #[test]
    fn test_shape_arabic_text_uses_script_fallback_when_font_missing() {
        ensure_test_fonts();
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("ar".to_string());
        options.font_fallback = true;

        let fixture = load_fixture("arabic_glyphs");
        let runs = backend.segment(&fixture.text, &options).unwrap();
        let fallback_target = Font::new("MissingArabicSupport", 48.0);
        let template_run = runs.first().expect("at least one run");
        let merged_run = TextRun {
            text: fixture.text.clone(),
            range: (0, fixture.text.len()),
            script: template_run.script.clone(),
            language: template_run.language.clone(),
            direction: template_run.direction,
            font: None,
        };
        let shaped = backend
            .shape(&merged_run, &fallback_target)
            .expect("Fallback shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        assert_eq!(glyph_ids, fixture.glyph_ids, "Fallback glyph ids changed");

        let resolved_font = shaped.font.as_ref().expect("fallback font present");
        assert_eq!(
            resolved_font.family, "NotoNaskhArabic-Regular",
            "expected Arabic fallback font to be Noto Naskh"
        );
    }

    #[test]
    fn test_shape_devanagari_text_uses_script_fallback_when_font_missing() {
        ensure_test_fonts();
        let backend = HarfBuzzBackend::new();
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("hi".to_string());
        options.font_fallback = true;

        let fixture = load_fixture("devanagari_glyphs");
        let runs = backend.segment(&fixture.text, &options).unwrap();
        let fallback_target = Font::new("MissingDevanagariSupport", 48.0);
        let template_run = runs.first().expect("at least one run");
        let merged_run = TextRun {
            text: fixture.text.clone(),
            range: (0, fixture.text.len()),
            script: template_run.script.clone(),
            language: template_run.language.clone(),
            direction: template_run.direction,
            font: None,
        };
        let shaped = backend
            .shape(&merged_run, &fallback_target)
            .expect("Fallback shaping succeeds");
        let glyph_ids: Vec<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        assert_eq!(glyph_ids, fixture.glyph_ids, "Fallback glyph ids changed");

        let resolved_font = shaped.font.as_ref().expect("fallback font present");
        assert_eq!(
            resolved_font.family, "NotoSansDevanagari-Regular",
            "expected Devanagari fallback font to be Noto Sans Devanagari"
        );
    }

    #[test]
    fn test_render_populates_glyph_cache() {
        let backend = HarfBuzzBackend::new();
        let font = fixture_font("NotoSans-Regular.ttf");
        let runs = backend
            .segment("Cache test", &SegmentOptions::default())
            .unwrap();
        let shaped = backend.shape(&runs[0], &font).unwrap();
        let mut options = RenderOptions::default();
        options.format = o4e_core::types::RenderFormat::Raw;

        backend.render(&shaped, &options).unwrap();
        let unique_glyphs: HashSet<u32> = shaped.glyphs.iter().map(|g| g.id).collect();
        let stats = backend.cache.stats();
        assert!(
            stats.glyph_count >= unique_glyphs.len(),
            "glyph cache should contain rendered glyphs"
        );
    }

    #[test]
    fn test_render_reuses_cached_glyphs() {
        let backend = HarfBuzzBackend::new();
        let font = fixture_font("NotoSans-Regular.ttf");
        let runs = backend
            .segment("Re-render", &SegmentOptions::default())
            .unwrap();
        let shaped = backend.shape(&runs[0], &font).unwrap();
        let mut options = RenderOptions::default();
        options.format = o4e_core::types::RenderFormat::Raw;

        backend.render(&shaped, &options).unwrap();
        let first = backend.cache.stats().glyph_count;

        backend.render(&shaped, &options).unwrap();
        let second = backend.cache.stats().glyph_count;

        assert_eq!(
            first, second,
            "glyph cache should not grow when re-rendering the same glyphs"
        );
    }

    #[test]
    fn test_clear_cache_empties_internal_layers() {
        ensure_test_fonts();
        let backend = HarfBuzzBackend::new();
        let font = fixture_font("NotoSans-Regular.ttf");
        let runs = backend
            .segment("Cache warmup", &SegmentOptions::default())
            .unwrap();
        let shaped = backend.shape(&runs[0], &font).unwrap();
        backend.render(&shaped, &RenderOptions::default()).unwrap();

        assert!(
            backend.cache.stats().glyph_count > 0,
            "glyph cache should be populated before clearing"
        );
        assert!(backend.hb_cache.read().len() > 0);
        assert!(backend.ttf_cache.read().len() > 0);
        assert!(backend.font_data_cache.read().len() > 0);

        backend.clear_cache();
        let stats = backend.cache.stats();
        assert!(stats.is_empty(), "cache stats after clear: {:?}", stats);
        assert_eq!(backend.hb_cache.read().len(), 0);
        assert_eq!(backend.ttf_cache.read().len(), 0);
        assert_eq!(backend.font_data_cache.read().len(), 0);
        assert_eq!(backend.family_path_cache.read().len(), 0);
    }
}
