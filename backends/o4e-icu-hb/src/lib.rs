// this_file: backends/o4e-icu-hb/src/lib.rs

//! ICU+HarfBuzz backend for cross-platform text rendering.

use harfbuzz_rs::{Face as HbFace, Font as HbFont, Language, Owned, Tag, UnicodeBuffer};
use icu_properties::{
    maps::{self, CodePointMapDataBorrowed},
    names::PropertyEnumToValueNameLinearMapperBorrowed,
    Script,
};
use icu_segmenter::{GraphemeClusterSegmenter, LineSegmenter, WordSegmenter};
use lru::LruCache;
use o4e_core::{
    types::Direction, Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions,
    RenderOutput, Result, SegmentOptions, ShapingResult, TextRun,
};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::Arc;
use tiny_skia::{Color, FillRule, Paint, PathBuilder, Pixmap, Transform};
use ttf_parser::{Face as TtfFace, OutlineBuilder};
use unicode_bidi::BidiInfo;

pub struct HarfBuzzBackend {
    cache: FontCache,
    hb_cache: RwLock<LruCache<String, Arc<Owned<HbFont<'static>>>>>,
    face_cache: RwLock<HashMap<String, Arc<Vec<u8>>>>,
    ttf_cache: RwLock<HashMap<String, Arc<TtfFace<'static>>>>,
    script_map: CodePointMapDataBorrowed<'static, Script>,
    script_name_mapper: PropertyEnumToValueNameLinearMapperBorrowed<'static, Script>,
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

#[derive(Clone, Copy)]
struct TextSlice {
    start: usize,
    end: usize,
    direction: Direction,
}

impl HarfBuzzBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
            hb_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            face_cache: RwLock::new(HashMap::new()),
            ttf_cache: RwLock::new(HashMap::new()),
            script_map: maps::script(),
            script_name_mapper: Script::enum_to_long_name_mapper(),
        }
    }

    fn compute_bidi_slices(&self, text: &str, resolve: bool) -> Vec<TextSlice> {
        if text.is_empty() {
            return Vec::new();
        }

        if !resolve {
            return vec![TextSlice {
                start: 0,
                end: text.len(),
                direction: Direction::LeftToRight,
            }];
        }

        let bidi = BidiInfo::new(text, None);
        let mut slices = Vec::new();

        for paragraph in &bidi.paragraphs {
            for run in paragraph.runs() {
                if run.range.start >= run.range.end {
                    continue;
                }
                let direction = if run.level.is_rtl() {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                };
                slices.push(TextSlice {
                    start: run.range.start,
                    end: run.range.end,
                    direction,
                });
            }
        }

        if slices.is_empty() {
            slices.push(TextSlice {
                start: 0,
                end: text.len(),
                direction: Direction::LeftToRight,
            });
        }

        slices
    }

    fn collect_runs_in_slice(
        &self,
        text: &str,
        slice: TextSlice,
        cluster_spans: &[(usize, usize)],
        line_breaks: &[usize],
        word_breaks: Option<&[usize]>,
        options: &SegmentOptions,
        language: &str,
        runs: &mut Vec<TextRun>,
    ) {
        if slice.end <= slice.start {
            return;
        }

        let slice_line_breaks: Vec<usize> = line_breaks
            .iter()
            .copied()
            .filter(|idx| *idx > slice.start && *idx < slice.end)
            .collect();
        let mut line_cursor = 0usize;

        let slice_word_breaks: Vec<usize> = word_breaks
            .unwrap_or(&[])
            .iter()
            .copied()
            .filter(|idx| *idx > slice.start && *idx < slice.end)
            .collect();
        let mut word_cursor = 0usize;

        let mut run_start = slice.start;
        let mut current_script: Option<Script> = None;

        for &(cluster_start, cluster_end) in cluster_spans {
            if cluster_end <= slice.start {
                continue;
            }
            if cluster_start >= slice.end {
                break;
            }

            let start = cluster_start.max(slice.start);
            let end = cluster_end.min(slice.end);
            if start >= end {
                continue;
            }

            let cluster_script = self.detect_script(&text[start..end]);
            let script_changed = options.script_itemize
                && self.is_significant_script(cluster_script)
                && current_script
                    .map(|existing| existing != cluster_script)
                    .unwrap_or(false);

            if script_changed && start > run_start {
                let script_for_run = current_script.unwrap_or(cluster_script);
                runs.push(self.build_run(
                    text,
                    run_start,
                    start,
                    script_for_run,
                    language,
                    slice.direction,
                ));
                run_start = start;
                current_script = None;
            }

            if current_script.is_none() && self.is_significant_script(cluster_script) {
                current_script = Some(cluster_script);
            }

            let mut boundary_hit = Self::hit_boundary(&slice_line_breaks, &mut line_cursor, end);
            if !boundary_hit && options.font_fallback && !slice_word_breaks.is_empty() {
                boundary_hit = Self::hit_boundary(&slice_word_breaks, &mut word_cursor, end);
            }

            if boundary_hit {
                if end > run_start {
                    let script_for_run = current_script.unwrap_or(cluster_script);
                    runs.push(self.build_run(
                        text,
                        run_start,
                        end,
                        script_for_run,
                        language,
                        slice.direction,
                    ));
                }
                run_start = end;
                current_script = None;
            }
        }

        if run_start < slice.end {
            let script_for_run =
                current_script.unwrap_or_else(|| self.detect_script(&text[run_start..slice.end]));
            runs.push(self.build_run(
                text,
                run_start,
                slice.end,
                script_for_run,
                language,
                slice.direction,
            ));
        }
    }

    fn detect_script(&self, fragment: &str) -> Script {
        for ch in fragment.chars() {
            let script = self.script_map.get(ch);
            if self.is_significant_script(script) {
                return script;
            }
        }
        Script::Common
    }

    fn build_run(
        &self,
        text: &str,
        start: usize,
        end: usize,
        script: Script,
        language: &str,
        direction: Direction,
    ) -> TextRun {
        TextRun {
            text: text[start..end].to_string(),
            range: (start, end),
            script: self.script_label(script),
            language: language.to_string(),
            direction,
            font: None,
        }
    }

    fn script_label(&self, script: Script) -> String {
        self.script_name_mapper
            .get(script)
            .unwrap_or("Unknown")
            .to_string()
    }

    fn is_significant_script(&self, script: Script) -> bool {
        !matches!(script, Script::Common | Script::Inherited | Script::Unknown)
    }

    fn hit_boundary(boundaries: &[usize], cursor: &mut usize, position: usize) -> bool {
        while *cursor < boundaries.len() && boundaries[*cursor] < position {
            *cursor += 1;
        }

        if *cursor < boundaries.len() && boundaries[*cursor] == position {
            *cursor += 1;
            return true;
        }

        false
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
}

impl Backend for HarfBuzzBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let grapheme_boundaries: Vec<usize> = GraphemeClusterSegmenter::new()
            .segment_str(text)
            .collect();
        if grapheme_boundaries.len() < 2 {
            return Ok(vec![TextRun {
                text: text.to_string(),
                range: (0, text.len()),
                script: "Common".to_string(),
                language: options.language.clone().unwrap_or_else(|| "en".to_string()),
                direction: Direction::LeftToRight,
                font: None,
            }]);
        }

        let cluster_spans: Vec<(usize, usize)> = grapheme_boundaries
            .windows(2)
            .map(|pair| (pair[0], pair[1]))
            .collect();

        let line_breaks: Vec<usize> = LineSegmenter::new_auto().segment_str(text).collect();
        let word_breaks: Vec<usize> = if options.font_fallback {
            WordSegmenter::new_auto().segment_str(text).collect()
        } else {
            Vec::new()
        };

        let language = options.language.clone().unwrap_or_else(|| "en".to_string());

        let slices = self.compute_bidi_slices(text, options.bidi_resolve);
        let mut runs = Vec::with_capacity(slices.len());

        for slice in slices {
            self.collect_runs_in_slice(
                text,
                slice,
                &cluster_spans,
                &line_breaks,
                if options.font_fallback {
                    Some(&word_breaks)
                } else {
                    None
                },
                options,
                &language,
                &mut runs,
            );
        }

        if runs.is_empty() {
            runs.push(self.build_run(
                text,
                0,
                text.len(),
                Script::Common,
                &language,
                Direction::LeftToRight,
            ));
        }

        Ok(runs)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        let hb_font = self.get_or_create_hb_font(font)?;

        // Create script tag from script name
        // Common scripts mapping - extend as needed
        let script_tag = match run.script.as_str() {
            "Latin" | "latin" => Tag::new('L', 'a', 't', 'n'),
            "Arabic" | "arabic" => Tag::new('A', 'r', 'a', 'b'),
            "Hebrew" | "hebrew" => Tag::new('H', 'e', 'b', 'r'),
            "Cyrillic" | "cyrillic" => Tag::new('C', 'y', 'r', 'l'),
            "Greek" | "greek" => Tag::new('G', 'r', 'e', 'k'),
            "Han" | "han" => Tag::new('H', 'a', 'n', 'i'),
            "Hiragana" | "hiragana" => Tag::new('H', 'i', 'r', 'a'),
            "Katakana" | "katakana" => Tag::new('K', 'a', 'n', 'a'),
            "Thai" | "thai" => Tag::new('T', 'h', 'a', 'i'),
            _ => Tag::new('L', 'a', 't', 'n'), // Default to Latin
        };

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
}
