// this_file: backends/o4e-mac/src/lib.rs

//! CoreText backend for macOS text rendering.

#![cfg(target_os = "macos")]

#[cfg(test)]
use camino::{Utf8Path, Utf8PathBuf};
use core_foundation::{
    attributed_string::CFMutableAttributedString,
    base::{CFRange, CFType, TCFType},
    dictionary::{CFDictionary, CFMutableDictionary},
    number::CFNumber,
    string::{CFString, CFStringRef},
};
use core_graphics::{
    color_space::CGColorSpace,
    context::{CGContext, CGTextDrawingMode},
    data_provider::CGDataProvider,
    font::{CGFont, CGGlyph},
    geometry::{CGPoint, CGRect, CGSize},
};
use core_text::{
    font::{self, new_from_name, CTFont, CTFontRef},
    font_descriptor::{
        self, kCTFontFamilyNameAttribute, kCTFontTraitsAttribute, kCTFontVariationAttribute,
        CTFontDescriptor,
    },
    line::CTLine,
    run::{CTRun, CTRunRef},
    string_attributes::{kCTFontAttributeName, kCTKernAttributeName, kCTLigatureAttributeName},
};
use lru::LruCache;
use o4e_core::{
    types::{AntialiasMode, FontSource, FontStyle, RenderFormat},
    Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions, RenderOptionsDiagnostics,
    RenderOutput, RenderSurface, Result, SegmentOptions, ShapingResult, TextRun,
};
use o4e_fontdb::FontDatabase;
use o4e_unicode::TextSegmenter;
use parking_lot::RwLock;
use std::borrow::Cow;
use std::num::NonZeroUsize;
use std::sync::Arc;

pub struct CoreTextBackend {
    cache: FontCache,
    ct_font_cache: RwLock<LruCache<String, Arc<CTFont>>>,
    shape_cache: RwLock<LruCache<String, Arc<ShapingResult>>>,
    segmenter: TextSegmenter,
    font_db: &'static FontDatabase,
}

struct ProviderData {
    bytes: Arc<[u8]>,
}

impl AsRef<[u8]> for ProviderData {
    fn as_ref(&self) -> &[u8] {
        &self.bytes
    }
}

impl CoreTextBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
            ct_font_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
            shape_cache: RwLock::new(LruCache::new(NonZeroUsize::new(256).unwrap())),
            segmenter: TextSegmenter::new(),
            font_db: FontDatabase::global(),
        }
    }

    fn font_cache_key(font: &Font) -> String {
        let mut variations: Vec<_> = font.variations.iter().collect();
        variations.sort_by(|a, b| a.0.cmp(b.0));
        let variation_str = variations
            .into_iter()
            .map(|(tag, value)| format!("{}={:.3}", tag, value))
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{}:{}:{}:{:?}:{}",
            font.family, font.size as u32, font.weight, font.style, variation_str
        )
    }

    fn shape_cache_key(text: &str, font: &Font) -> String {
        format!("{}::{}", text, Self::font_cache_key(font))
    }

    fn get_or_create_ct_font(&self, font: &Font) -> Result<Arc<CTFont>> {
        let cache_key = Self::font_cache_key(font);
        {
            let mut cache = self.ct_font_cache.write();
            if let Some(ct_font) = cache.get(&cache_key) {
                return Ok(ct_font.clone());
            }
        }

        let ct_font = self.build_ct_font(font)?;
        let ct_font = Arc::new(ct_font);
        {
            let mut cache = self.ct_font_cache.write();
            cache.push(cache_key, ct_font.clone());
        }
        Ok(ct_font)
    }

    fn cache_ct_font_instance(&self, font: &Font, ct_font: Arc<CTFont>) {
        let cache_key = Self::font_cache_key(font);
        let mut cache = self.ct_font_cache.write();
        cache.push(cache_key, ct_font);
    }

    fn build_ct_font(&self, font: &Font) -> Result<CTFont> {
        if matches!(font.source, FontSource::Family(_)) {
            if let Ok(ct_font) = new_from_name(&font.family, font.size as f64) {
                return Ok(ct_font);
            }
        }

        if let Some(ct_font) = self.try_load_ct_font_from_source(font)? {
            return Ok(ct_font);
        }

        let descriptor = Self::descriptor_for_font(font);
        Ok(font::new_from_descriptor(&descriptor, font.size as f64))
    }

    fn try_load_ct_font_from_source(&self, font: &Font) -> Result<Option<CTFont>> {
        match font.source {
            FontSource::Family(_) => Ok(None),
            _ => {
                let handle = self.font_db.resolve(font)?;
                let provider_data = Arc::new(ProviderData {
                    bytes: handle.bytes.clone(),
                });
                let provider = CGDataProvider::from_buffer(provider_data);
                let cg_font = CGFont::from_data_provider(provider).map_err(|_| {
                    O4eError::render(format!("Failed to create CGFont from '{}'", handle.family))
                })?;
                Ok(Some(font::new_from_CGFont(&cg_font, font.size as f64)))
            }
        }
    }

    fn descriptor_for_font(font: &Font) -> CTFontDescriptor {
        let mut attributes = CFMutableDictionary::<CFString, CFType>::new();
        let family_key = unsafe { CFString::wrap_under_get_rule(kCTFontFamilyNameAttribute) };
        let family_value = CFString::new(&font.family);
        let family_cf = unsafe { CFType::wrap_under_get_rule(family_value.as_CFTypeRef()) };
        attributes.set(family_key, family_cf);

        if let Some(traits) = Self::traits_dictionary(font) {
            let traits_key = unsafe { CFString::wrap_under_get_rule(kCTFontTraitsAttribute) };
            let traits_value = unsafe { CFType::wrap_under_get_rule(traits.as_CFTypeRef()) };
            attributes.set(traits_key, traits_value);
        }

        if let Some(vars) = Self::variation_dictionary(font) {
            let var_key = unsafe { CFString::wrap_under_get_rule(kCTFontVariationAttribute) };
            let var_value = unsafe { CFType::wrap_under_get_rule(vars.as_CFTypeRef()) };
            attributes.set(var_key, var_value);
        }

        let dict = attributes.to_immutable();
        font_descriptor::new_from_attributes(&dict)
    }

    fn traits_dictionary(font: &Font) -> Option<CFDictionary<CFString, CFNumber>> {
        let mut traits = CFMutableDictionary::<CFString, CFNumber>::new();
        let weight_key =
            unsafe { CFString::wrap_under_get_rule(font_descriptor::kCTFontWeightTrait) };
        traits.set(
            weight_key,
            CFNumber::from(Self::normalized_weight(font.weight)),
        );

        let slant_key =
            unsafe { CFString::wrap_under_get_rule(font_descriptor::kCTFontSlantTrait) };
        traits.set(slant_key, CFNumber::from(Self::slant_value(font.style)));

        Some(traits.to_immutable())
    }

    fn variation_dictionary(font: &Font) -> Option<CFDictionary<CFNumber, CFNumber>> {
        if font.variations.is_empty() {
            return None;
        }

        let mut dict = CFMutableDictionary::<CFNumber, CFNumber>::new();
        for (tag, value) in &font.variations {
            if let Some(axis) = Self::axis_tag_to_number(tag) {
                dict.set(axis, CFNumber::from(*value as f64));
            }
        }

        if dict.is_empty() {
            None
        } else {
            Some(dict.to_immutable())
        }
    }

    fn axis_tag_to_number(tag: &str) -> Option<CFNumber> {
        if tag.is_empty() {
            return None;
        }

        let mut buf = [b' '; 4];
        for (idx, byte) in tag.as_bytes().iter().take(4).enumerate() {
            buf[idx] = *byte;
        }
        let value = u32::from_be_bytes(buf);
        Some(CFNumber::from(value as i64))
    }

    fn normalized_weight(weight: u16) -> f64 {
        let clamped = weight.clamp(1, 1000) as f64;
        ((clamped - 400.0) / 400.0).clamp(-1.0, 1.0)
    }

    fn slant_value(style: FontStyle) -> f64 {
        match style {
            FontStyle::Normal => 0.0,
            FontStyle::Italic => -1.0,
            FontStyle::Oblique => -0.5,
        }
    }

    fn create_attributed_string(
        &self,
        text: &str,
        font: &Font,
        ct_font: &CTFont,
    ) -> CFMutableAttributedString {
        let cf_string = CFString::new(text);
        let mut attributed_string = CFMutableAttributedString::new();
        attributed_string.replace_str(&cf_string, CFRange::init(0, 0));

        let range = CFRange::init(0, attributed_string.char_len());
        attributed_string.set_attribute(range, unsafe { kCTFontAttributeName }, ct_font);
        self.apply_feature_attributes(&mut attributed_string, range, font);
        attributed_string
    }

    fn apply_feature_attributes(
        &self,
        attributed_string: &mut CFMutableAttributedString,
        range: CFRange,
        font: &Font,
    ) {
        if let Some(enabled) = font.features.tags.get("liga") {
            let ligature_value = CFNumber::from(if *enabled { 1 } else { 0 });
            attributed_string.set_attribute(
                range,
                unsafe { kCTLigatureAttributeName },
                &ligature_value,
            );
        }

        if let Some(enabled) = font.features.tags.get("kern") {
            if !*enabled {
                let zero = CFNumber::from(0.0f64);
                attributed_string.set_attribute(range, unsafe { kCTKernAttributeName }, &zero);
            }
        }
    }

    fn effective_font_for_run(
        &self,
        run: &TextRun,
        fallback: &Font,
    ) -> Result<(Font, Arc<CTFont>)> {
        let mut desired_font = run.font.clone().unwrap_or_else(|| fallback.clone());
        desired_font.size = fallback.size;

        let ct_font = self.get_or_create_ct_font(&desired_font)?;
        if run.text.is_empty() || Self::font_supports_text(&ct_font, &run.text) {
            return Ok((desired_font, ct_font));
        }

        if let Some(fallback_font) = self.fallback_ct_font(&ct_font, &run.text, &run.language) {
            if fallback_font.postscript_name() != ct_font.postscript_name() {
                let resolved_font = Self::font_from_ct_font(&desired_font, &fallback_font);
                let arc = Arc::new(fallback_font);
                self.cache_ct_font_instance(&resolved_font, arc.clone());
                return Ok((resolved_font, arc));
            }
        }

        Ok((desired_font, ct_font))
    }

    fn font_supports_text(ct_font: &CTFont, text: &str) -> bool {
        if text.is_empty() {
            return true;
        }

        let utf16: Vec<u16> = text.encode_utf16().collect();
        if utf16.is_empty() {
            return true;
        }

        let mut glyphs = vec![0 as CGGlyph; utf16.len()];
        unsafe {
            ct_font.get_glyphs_for_characters(utf16.as_ptr(), glyphs.as_mut_ptr(), utf16.len() as _)
        }
    }

    fn fallback_ct_font(&self, ct_font: &CTFont, text: &str, language: &str) -> Option<CTFont> {
        if text.is_empty() {
            return None;
        }

        let cf_text = CFString::new(text);
        let range = CFRange::init(0, cf_text.char_len());

        unsafe {
            let font_ref = if language.is_empty() {
                CTFontCreateForString(
                    ct_font.as_concrete_TypeRef(),
                    cf_text.as_concrete_TypeRef(),
                    range,
                )
            } else {
                let cf_language = CFString::new(language);
                CTFontCreateForStringWithLanguage(
                    ct_font.as_concrete_TypeRef(),
                    cf_text.as_concrete_TypeRef(),
                    range,
                    cf_language.as_concrete_TypeRef(),
                )
            };

            if font_ref.is_null() {
                None
            } else {
                Some(CTFont::wrap_under_create_rule(font_ref))
            }
        }
    }

    fn font_from_ct_font(base: &Font, ct_font: &CTFont) -> Font {
        let mut resolved = base.clone();
        resolved.family = ct_font.family_name();
        resolved.variations.clear();
        resolved
    }

    fn collect_run_glyphs(run: &CTRun, glyphs: &mut Vec<Glyph>) -> f32 {
        let glyph_ids: Cow<[CGGlyph]> = run.glyphs();
        if glyph_ids.is_empty() {
            return 0.0;
        }

        let positions: Cow<[CGPoint]> = run.positions();
        let indices: Cow<[isize]> = run.string_indices();
        let advances = Self::run_advances(run);

        let mut advance_sum = 0.0f32;
        for idx in 0..glyph_ids.len() {
            let position = positions
                .get(idx)
                .copied()
                .unwrap_or(CGPoint { x: 0.0, y: 0.0 });
            let cluster = indices.get(idx).copied().unwrap_or(0);
            let advance = advances
                .get(idx)
                .map(|size| size.width as f32)
                .unwrap_or(0.0);

            glyphs.push(Glyph {
                id: glyph_ids[idx] as u32,
                cluster: cluster.max(0) as u32,
                x: position.x as f32,
                y: position.y as f32,
                advance,
            });
            advance_sum += advance;
        }

        advance_sum
    }

    fn run_advances(run: &CTRun) -> Vec<CGSize> {
        let glyph_count = run.glyph_count();
        if glyph_count <= 0 {
            return Vec::new();
        }

        let mut advances = vec![
            CGSize {
                width: 0.0,
                height: 0.0,
            };
            glyph_count as usize
        ];
        unsafe {
            CTRunGetAdvances(
                run.as_concrete_TypeRef(),
                CFRange::init(0, 0),
                advances.as_mut_ptr(),
            );
        }
        advances
    }

    fn configure_antialias(context: &CGContext, mode: AntialiasMode) {
        match mode {
            AntialiasMode::None => {
                context.set_should_antialias(false);
                context.set_should_smooth_fonts(false);
            }
            AntialiasMode::Grayscale => {
                context.set_should_antialias(true);
                context.set_should_smooth_fonts(false);
            }
            AntialiasMode::Subpixel => {
                context.set_should_antialias(true);
                context.set_should_smooth_fonts(true);
            }
        }
    }
}

impl Backend for CoreTextBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        self.segmenter.segment(text, options)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        let (resolved_font, ct_font) = self.effective_font_for_run(run, font)?;
        let cache_key = Self::shape_cache_key(&run.text, &resolved_font);
        {
            let mut cache = self.shape_cache.write();
            if let Some(result) = cache.get(&cache_key) {
                return Ok((**result).clone());
            }
        }

        let attributed_string = self.create_attributed_string(&run.text, &resolved_font, &ct_font);
        let line = CTLine::new_with_attributed_string(attributed_string.as_concrete_TypeRef());

        let mut glyphs = Vec::new();
        let mut advance = 0.0f32;
        for ct_run in line.glyph_runs().iter() {
            advance += Self::collect_run_glyphs(&ct_run, &mut glyphs);
        }

        let bbox = o4e_core::utils::calculate_bbox(&glyphs);
        let result = ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance,
            bbox,
            font: Some(resolved_font.clone()),
            direction: run.direction,
        };
        let result = Arc::new(result);

        {
            let mut cache = self.shape_cache.write();
            cache.push(cache_key, result.clone());
        }

        Ok((*result).clone())
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        RenderOptionsDiagnostics::new(self.name(), shaped, options).log();
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

        let ct_font = self.get_or_create_ct_font(font)?;
        let padding = options.padding as f32;
        let content_width = shaped.bbox.width.max(shaped.advance).max(1.0);
        let content_height = shaped
            .bbox
            .height
            .max((ct_font.ascent() + ct_font.descent()) as f32)
            .max(1.0);
        let width = (content_width + padding * 2.0).ceil() as usize;
        let height = (content_height + padding * 2.0).ceil() as usize;

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

        Self::configure_antialias(&context, options.antialias);

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

        // Flip coordinate system (CoreGraphics uses bottom-left origin)
        context.translate(0.0, height as f64);
        context.scale(1.0, -1.0);

        // Calculate baseline position
        let baseline_y = padding as f64 + ct_font.ascent();

        let glyph_ids: Vec<CGGlyph> = shaped
            .glyphs
            .iter()
            .map(|glyph| glyph.id.min(u16::MAX as u32) as CGGlyph)
            .collect();
        let glyph_positions: Vec<CGPoint> = shaped
            .glyphs
            .iter()
            .map(|glyph| CGPoint {
                x: glyph.x as f64,
                y: glyph.y as f64,
            })
            .collect();

        context.save();
        context.translate(padding as f64, baseline_y);
        context.set_text_drawing_mode(CGTextDrawingMode::CGTextFill);
        ct_font.draw_glyphs(&glyph_ids, &glyph_positions, context.clone());
        context.restore();

        if options.format == RenderFormat::Svg {
            let svg_options = o4e_core::types::SvgOptions::default();
            let renderer = o4e_render::SvgRenderer::new(&svg_options);
            let svg = renderer.render(&shaped, &svg_options);
            return Ok(RenderOutput::Svg(svg));
        }

        let surface = RenderSurface::from_rgba(width as u32, height as u32, buffer, true);
        surface.into_render_output(options.format)
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

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    fn CTRunGetAdvances(run: CTRunRef, range: CFRange, buffer: *mut CGSize);
    fn CTFontCreateForString(
        current_font: CTFontRef,
        string: CFStringRef,
        range: CFRange,
    ) -> CTFontRef;
    fn CTFontCreateForStringWithLanguage(
        current_font: CTFontRef,
        string: CFStringRef,
        range: CFRange,
        language: CFStringRef,
    ) -> CTFontRef;
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
    fn test_shape_glyph_advances_match_total_for_latin_text() {
        let backend = CoreTextBackend::new();
        let font = Font::new("Helvetica", 40.0);
        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!("Skipping latin glyph test; Helvetica not available");
            return;
        }

        let runs = backend
            .segment("Glyph extraction proof", &SegmentOptions::default())
            .unwrap();
        let run = runs.first().expect("latin run");
        let shaped = backend.shape(run, &font).unwrap();
        assert!(
            !shaped.glyphs.is_empty(),
            "Expected glyphs for latin shaping"
        );

        let sum: f32 = shaped.glyphs.iter().map(|g| g.advance).sum();
        assert!(
            (sum - shaped.advance).abs() < 0.25,
            "Glyph advances ({sum}) should equal total advance ({})",
            shaped.advance
        );
    }

    #[test]
    fn test_shape_glyph_advances_match_total_for_arabic_text() {
        let backend = CoreTextBackend::new();
        let font = Font::new("Geeza Pro", 42.0);
        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!("Skipping arabic glyph test; Geeza Pro not available");
            return;
        }

        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        let runs = backend.segment("مرحبا بالعالم", &options).unwrap();
        let arabic_run = runs
            .iter()
            .find(|run| run.script == "Arabic")
            .expect("Arabic run missing");
        let shaped = backend.shape(arabic_run, &font).unwrap();
        assert!(
            !shaped.glyphs.is_empty(),
            "Expected glyphs for Arabic shaping"
        );

        let sum: f32 = shaped.glyphs.iter().map(|g| g.advance).sum();
        assert!(
            (sum - shaped.advance).abs() < 0.5,
            "Glyph advances ({sum}) should equal total advance ({})",
            shaped.advance
        );
    }

    #[test]
    fn test_shape_advance_matches_coretext_bounds() {
        let backend = CoreTextBackend::new();
        let font = Font::new("Helvetica", 36.0);
        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!("Skipping typographic bounds test; Helvetica not available");
            return;
        }

        let runs = backend
            .segment("Advance width check", &SegmentOptions::default())
            .unwrap();
        let run = runs.first().expect("run");
        let shaped = backend.shape(run, &font).unwrap();

        let ct_font = backend.get_or_create_ct_font(&font).unwrap();
        let cf_string = CFString::new(&run.text);
        let mut attributed = CFMutableAttributedString::new();
        attributed.replace_str(&cf_string, CFRange::init(0, 0));
        let range = CFRange::init(0, attributed.char_len());
        attributed.set_attribute(range, unsafe { kCTFontAttributeName }, &*ct_font);
        let line = CTLine::new_with_attributed_string(attributed.as_concrete_TypeRef());
        let bounds = line.get_typographic_bounds();
        assert!(
            (shaped.advance - bounds.width as f32).abs() < 0.5,
            "Expected shaped advance {} to match CTLine width {}",
            shaped.advance,
            bounds.width
        );
    }

    #[test]
    fn test_coretext_png_snapshot_matches_expected() {
        let backend = CoreTextBackend::new();
        let font = Font::new("Helvetica", 44.0);
        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!("Skipping snapshot test; Helvetica not available");
            return;
        }

        let runs = backend
            .segment("Snapshot", &SegmentOptions::default())
            .unwrap();
        let run = runs.first().expect("run");
        let shaped = backend.shape(run, &font).unwrap();

        let mut render_options = RenderOptions::default();
        render_options.format = RenderFormat::Png;
        render_options.background = "#FFFFFFFF".to_string();
        render_options.color = "#000000FF".to_string();
        render_options.antialias = AntialiasMode::Grayscale;
        render_options.padding = 8;

        let output = backend.render(&shaped, &render_options).unwrap();
        let RenderOutput::Png(actual) = output else {
            panic!("Expected PNG output");
        };

        let snapshot_path: Utf8PathBuf = Utf8Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/expected/coretext/latin_snapshot.png");
        if !snapshot_path.exists() {
            std::fs::create_dir_all(
                snapshot_path
                    .parent()
                    .expect("snapshot parent must exist")
                    .as_std_path(),
            )
            .expect("create snapshot dir");
            std::fs::write(snapshot_path.as_std_path(), &actual).expect("write snapshot");
            panic!(
                "Snapshot {} missing; generated a fresh copy. Re-run tests.",
                snapshot_path
            );
        }

        let expected = std::fs::read(snapshot_path.as_std_path()).expect("read snapshot");
        assert_eq!(
            actual, expected,
            "Rendered PNG should match snapshot at {}",
            snapshot_path
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

    #[test]
    fn test_clear_cache_drops_ctfont_entries() {
        let backend = CoreTextBackend::new();
        let font = Font::new("Helvetica", 32.0);
        if backend.get_or_create_ct_font(&font).is_err() {
            eprintln!("Skipping clear_cache test; Helvetica not available");
            return;
        }

        let runs = backend
            .segment("Cache warmup", &SegmentOptions::default())
            .unwrap();
        let shaped = backend.shape(&runs[0], &font).unwrap();
        backend.render(&shaped, &RenderOptions::default()).unwrap();

        assert!(
            backend.ct_font_cache.read().len() > 0,
            "ct_font_cache should populate after a render"
        );
        backend.clear_cache();
        assert_eq!(
            backend.ct_font_cache.read().len(),
            0,
            "ct_font_cache should be empty after clear_cache"
        );
    }
}
