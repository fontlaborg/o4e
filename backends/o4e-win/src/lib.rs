// this_file: backends/o4e-win/src/lib.rs

//! DirectWrite backend for Windows text rendering.

#![cfg(target_os = "windows")]

use o4e_core::{
    types::{AntialiasMode, Direction, RenderFormat},
    Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions, RenderOutput, RenderSurface,
    Result, SegmentOptions, ShapingResult, TextRun,
};

use windows::Win32::Graphics::DirectWrite::{
    IDWriteTextAnalysisSink_Impl, IDWriteTextAnalysisSource_Impl,
};
use windows::{
    core::{ComObject, PCWSTR, *},
    Win32::{
        Foundation::*,
        Graphics::{Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Imaging::*},
        System::Com::*,
    },
};

use lru::LruCache;
use parking_lot::RwLock;
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
    fmt::Write,
    mem::ManuallyDrop,
    num::NonZeroUsize,
    slice,
    sync::Arc,
};

#[derive(Clone)]
struct ScriptRun {
    start: u32,
    length: u32,
    analysis: DWRITE_SCRIPT_ANALYSIS,
}

#[derive(Clone)]
struct BidiRun {
    start: u32,
    length: u32,
    level: u8,
}

struct AnalysisResults {
    script_runs: Vec<ScriptRun>,
    bidi_runs: Vec<BidiRun>,
    line_breaks: Vec<DWRITE_LINE_BREAKPOINT>,
    text_len: u32,
}

struct FeatureBindings {
    features: Vec<DWRITE_FONT_FEATURE>,
    typographic: Vec<DWRITE_TYPOGRAPHIC_FEATURES>,
    pointers: Vec<*const DWRITE_TYPOGRAPHIC_FEATURES>,
    ranges: Vec<u32>,
}

impl FeatureBindings {
    fn new(font: &Font, text_len: u32) -> Option<Self> {
        if font.features.tags.is_empty() {
            return None;
        }

        let mut ordered: Vec<_> = font.features.tags.iter().collect();
        ordered.sort_by(|a, b| a.0.cmp(b.0));
        let mut features = Vec::with_capacity(ordered.len());
        for (tag, enabled) in ordered {
            if let Some(raw_tag) = ot_tag(tag) {
                features.push(DWRITE_FONT_FEATURE {
                    nameTag: DWRITE_FONT_FEATURE_TAG(raw_tag),
                    parameter: if *enabled { 1 } else { 0 },
                });
            }
        }
        if features.is_empty() {
            return None;
        }

        let mut typographic = vec![DWRITE_TYPOGRAPHIC_FEATURES {
            features: features.as_mut_ptr(),
            featureCount: features.len() as u32,
        }];
        let pointers = vec![typographic.as_ptr()];
        let ranges = vec![text_len];
        Some(Self {
            features,
            typographic,
            pointers,
            ranges,
        })
    }
}

impl AnalysisResults {
    fn script_at(&self, position: u32) -> Option<&ScriptRun> {
        self.script_runs
            .iter()
            .find(|run| position >= run.start && position < run.start + run.length)
    }

    fn bidi_at(&self, position: u32) -> Option<&BidiRun> {
        self.bidi_runs
            .iter()
            .find(|run| position >= run.start && position < run.start + run.length)
    }
}

#[windows::core::implement(IDWriteTextAnalysisSource, IDWriteTextAnalysisSink)]
struct TextAnalysisBridge {
    text: Vec<u16>,
    locale: Vec<u16>,
    direction: DWRITE_READING_DIRECTION,
    script_runs: RefCell<Vec<ScriptRun>>,
    bidi_runs: RefCell<Vec<BidiRun>>,
    line_breaks: RefCell<Vec<DWRITE_LINE_BREAKPOINT>>,
}

impl TextAnalysisBridge {
    fn new(text: Vec<u16>, mut locale: Vec<u16>, direction: DWRITE_READING_DIRECTION) -> Self {
        if !locale.ends_with(&[0]) {
            locale.push(0);
        }
        let line_breaks = vec![DWRITE_LINE_BREAKPOINT::default(); text.len()];
        Self {
            text,
            locale,
            direction,
            script_runs: RefCell::new(Vec::new()),
            bidi_runs: RefCell::new(Vec::new()),
            line_breaks: RefCell::new(line_breaks),
        }
    }

    fn text_len(&self) -> u32 {
        self.text.len() as u32
    }

    fn results(&self) -> AnalysisResults {
        AnalysisResults {
            script_runs: self.script_runs.borrow().clone(),
            bidi_runs: self.bidi_runs.borrow().clone(),
            line_breaks: self.line_breaks.borrow().clone(),
            text_len: self.text_len(),
        }
    }
}

#[allow(non_snake_case)]
impl IDWriteTextAnalysisSource_Impl for TextAnalysisBridge_Impl {
    fn GetTextAtPosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        let len = this.text_len();
        unsafe {
            if !textstring.is_null() {
                if textposition >= len {
                    *textstring = std::ptr::null_mut();
                    if !textlength.is_null() {
                        *textlength = 0;
                    }
                } else {
                    *textstring = this.text.as_ptr().add(textposition as usize) as *mut u16;
                    if !textlength.is_null() {
                        *textlength = len - textposition;
                    }
                }
            }
        }
        Ok(())
    }

    fn GetTextBeforePosition(
        &self,
        textposition: u32,
        textstring: *mut *mut u16,
        textlength: *mut u32,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        let len = this.text_len();
        let clamped = textposition.min(len);
        unsafe {
            if !textstring.is_null() {
                if clamped == 0 {
                    *textstring = std::ptr::null_mut();
                    if !textlength.is_null() {
                        *textlength = 0;
                    }
                } else {
                    *textstring = this.text.as_ptr() as *mut u16;
                    if !textlength.is_null() {
                        *textlength = clamped;
                    }
                }
            }
        }
        Ok(())
    }

    fn GetParagraphReadingDirection(&self) -> DWRITE_READING_DIRECTION {
        let this: &TextAnalysisBridge = self;
        this.direction
    }

    fn GetLocaleName(
        &self,
        _textposition: u32,
        textlength: *mut u32,
        localename: *mut *mut u16,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        unsafe {
            if !localename.is_null() {
                *localename = this.locale.as_ptr() as *mut u16;
            }
            if !textlength.is_null() {
                *textlength = this.text_len();
            }
        }
        Ok(())
    }

    fn GetNumberSubstitution(
        &self,
        _textposition: u32,
        textlength: *mut u32,
        numbersubstitution: *mut Option<IDWriteNumberSubstitution>,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        unsafe {
            if !numbersubstitution.is_null() {
                *numbersubstitution = None;
            }
            if !textlength.is_null() {
                *textlength = this.text_len();
            }
        }
        Ok(())
    }
}

#[allow(non_snake_case)]
impl IDWriteTextAnalysisSink_Impl for TextAnalysisBridge_Impl {
    fn SetScriptAnalysis(
        &self,
        textposition: u32,
        textlength: u32,
        scriptanalysis: *const DWRITE_SCRIPT_ANALYSIS,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        if let Some(analysis) = unsafe { scriptanalysis.as_ref() } {
            this.script_runs.borrow_mut().push(ScriptRun {
                start: textposition,
                length: textlength,
                analysis: *analysis,
            });
        }
        Ok(())
    }

    fn SetLineBreakpoints(
        &self,
        textposition: u32,
        textlength: u32,
        linebreakpoints: *const DWRITE_LINE_BREAKPOINT,
    ) -> windows::core::Result<()> {
        if textlength == 0 {
            return Ok(());
        }
        let this: &TextAnalysisBridge = self;
        let start = textposition as usize;
        let end = (textposition + textlength).min(this.text_len()) as usize;
        let source = unsafe { slice::from_raw_parts(linebreakpoints, textlength as usize) };
        let mut targets = this.line_breaks.borrow_mut();
        if end <= targets.len() && source.len() == end - start {
            targets[start..end].copy_from_slice(source);
        }
        Ok(())
    }

    fn SetBidiLevel(
        &self,
        textposition: u32,
        textlength: u32,
        _explicitlevel: u8,
        resolvedlevel: u8,
    ) -> windows::core::Result<()> {
        let this: &TextAnalysisBridge = self;
        this.bidi_runs.borrow_mut().push(BidiRun {
            start: textposition,
            length: textlength,
            level: resolvedlevel,
        });
        Ok(())
    }

    fn SetNumberSubstitution(
        &self,
        _textposition: u32,
        _textlength: u32,
        _numbersubstitution: Option<&IDWriteNumberSubstitution>,
    ) -> windows::core::Result<()> {
        Ok(())
    }
}

pub struct DirectWriteBackend {
    dwrite_factory: IDWriteFactory,
    d2d_factory: ID2D1Factory,
    wic_factory: IWICImagingFactory,
    text_analyzer: IDWriteTextAnalyzer1,
    cache: FontCache,
    font_cache: RwLock<LruCache<String, IDWriteFontFace>>,
    shape_cache: RwLock<LruCache<String, Arc<ShapingResult>>>,
}

// Safety: DirectWrite interfaces are thread-safe when used correctly
unsafe impl Send for DirectWriteBackend {}
unsafe impl Sync for DirectWriteBackend {}

impl DirectWriteBackend {
    pub fn new() -> Result<Self> {
        unsafe {
            // Initialize COM
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // Create DirectWrite factory
            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            // Create Direct2D factory
            let d2d_factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None)?;

            // Create WIC factory for image processing
            let wic_factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;
            let text_analyzer = dwrite_factory
                .CreateTextAnalyzer()
                .map_err(|e| O4eError::other(format!("CreateTextAnalyzer failed: {e}")))?
                .cast()
                .map_err(|e| O4eError::other(format!("TextAnalyzer cast failed: {e}")))?;

            Ok(Self {
                dwrite_factory,
                d2d_factory,
                wic_factory,
                text_analyzer,
                cache: FontCache::new(512),
                font_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
                shape_cache: RwLock::new(LruCache::new(NonZeroUsize::new(256).unwrap())),
            })
        }
    }

    fn get_or_create_font_face(&self, font: &Font) -> Result<IDWriteFontFace> {
        let cache_key = Self::font_cache_key(font);

        // Check cache
        {
            let mut cache = self.font_cache.write();
            if let Some(font_face) = cache.get(&cache_key) {
                return Ok(font_face.clone());
            }
        }

        unsafe {
            // Get system font collection
            let font_collection = self.dwrite_factory.GetSystemFontCollection(false)?;

            // Find font family
            let family_name = HSTRING::from(&font.family);
            let mut index = 0u32;
            let mut exists = BOOL::default();
            font_collection.FindFamilyName(&family_name, &mut index, &mut exists)?;

            if !exists.as_bool() {
                return Err(O4eError::FontNotFound {
                    name: font.family.clone(),
                }
                .into());
            }

            // Get font family
            let font_family = font_collection.GetFontFamily(index)?;

            // Get font with specified weight and style
            let weight = DWRITE_FONT_WEIGHT(font.weight as i32);
            let style = DWRITE_FONT_STYLE_NORMAL;
            let stretch = DWRITE_FONT_STRETCH_NORMAL;

            let dwrite_font = font_family.GetFirstMatchingFont(weight, stretch, style)?;

            // Create font face
            let mut font_face = dwrite_font.CreateFontFace()?;
            if let Some(custom_face) = self.build_variation_face(&font_face, font)? {
                font_face = custom_face;
            }

            // Cache it
            {
                let mut cache = self.font_cache.write();
                cache.push(cache_key, font_face.clone());
            }

            Ok(font_face)
        }
    }

    fn font_cache_key(font: &Font) -> String {
        let mut key = format!(
            "{}:{}:{}:{:?}",
            font.family, font.size, font.weight, font.style
        );
        let mut variation_keys: Vec<_> = font.variations.iter().collect();
        variation_keys.sort_by(|a, b| a.0.cmp(b.0));
        for (tag, value) in variation_keys {
            let _ = write!(key, ":{tag}={value}");
        }
        key
    }

    fn build_variation_face(
        &self,
        base_face: &IDWriteFontFace,
        font: &Font,
    ) -> Result<Option<IDWriteFontFace>> {
        if font.variations.is_empty() {
            return Ok(None);
        }
        let variation_requests = variation_overrides(font);
        if variation_requests.is_empty() {
            return Ok(None);
        }

        let face5: IDWriteFontFace5 = match base_face.cast() {
            Ok(face5) => face5,
            Err(_) => return Ok(None),
        };
        let resource = face5
            .GetFontResource()
            .map_err(|e| O4eError::other(format!("GetFontResource failed: {e}")))?;
        let axis_count = unsafe { resource.GetFontAxisCount() };
        if axis_count == 0 {
            return Ok(None);
        }

        let mut axis_values = vec![DWRITE_FONT_AXIS_VALUE::default(); axis_count as usize];
        unsafe {
            resource
                .GetDefaultFontAxisValues(&mut axis_values)
                .map_err(|e| O4eError::other(format!("GetDefaultFontAxisValues failed: {e}")))?;
        }

        let mut touched = false;
        for value in axis_values.iter_mut() {
            if let Some(request) = variation_requests.get(&value.axisTag.0) {
                value.value = *request;
                touched = true;
            }
        }
        if !touched {
            return Ok(None);
        }

        let custom_face5 = unsafe {
            resource
                .CreateFontFace(DWRITE_FONT_SIMULATIONS_NONE, &axis_values)
                .map_err(|e| {
                    O4eError::other(format!("CreateFontFace with variations failed: {e}"))
                })?
        };
        let custom_face: IDWriteFontFace = custom_face5
            .cast()
            .map_err(|e| O4eError::other(format!("FontFace5 cast failed: {e}")))?;
        Ok(Some(custom_face))
    }

    fn analyze_text(
        &self,
        text_units: Vec<u16>,
        locale: &str,
        direction: DWRITE_READING_DIRECTION,
        include_bidi: bool,
        include_breaks: bool,
    ) -> Result<AnalysisResults> {
        let bridge = ComObject::new(TextAnalysisBridge::new(
            text_units,
            locale.encode_utf16().collect(),
            direction,
        ));
        let source: IDWriteTextAnalysisSource = bridge.clone().into();
        let sink: IDWriteTextAnalysisSink = bridge.clone().into();
        let len = (&bridge as &TextAnalysisBridge).text_len();

        unsafe {
            self.text_analyzer
                .AnalyzeScript(&source, 0, len, &sink)
                .map_err(|e| O4eError::segmentation(format!("AnalyzeScript failed: {e}")))?;
            if include_bidi {
                self.text_analyzer
                    .AnalyzeBidi(&source, 0, len, &sink)
                    .map_err(|e| O4eError::segmentation(format!("AnalyzeBidi failed: {e}")))?;
            }
            if include_breaks {
                self.text_analyzer
                    .AnalyzeLineBreakpoints(&source, 0, len, &sink)
                    .map_err(|e| {
                        O4eError::segmentation(format!("AnalyzeLineBreakpoints failed: {e}"))
                    })?;
            }
        }

        Ok((&bridge as &TextAnalysisBridge).results())
    }

    fn script_tag(&self, analysis: &DWRITE_SCRIPT_ANALYSIS) -> Result<String> {
        let mut props = DWRITE_SCRIPT_PROPERTIES::default();
        unsafe {
            self.text_analyzer
                .GetScriptProperties(*analysis, &mut props)
                .map_err(|e| O4eError::segmentation(format!("GetScriptProperties failed: {e}")))?;
        }
        Ok(iso_script_tag(props.isoScriptCode))
    }

    fn collect_boundaries(
        &self,
        text: &str,
        analysis: &AnalysisResults,
        options: &SegmentOptions,
    ) -> Vec<u32> {
        let mut boundaries: BTreeSet<u32> = BTreeSet::new();
        boundaries.insert(0);
        boundaries.insert(analysis.text_len);

        if options.script_itemize {
            for run in &analysis.script_runs {
                boundaries.insert(run.start);
                boundaries.insert(run.start + run.length);
            }
        }

        if options.bidi_resolve {
            for run in &analysis.bidi_runs {
                boundaries.insert(run.start);
                boundaries.insert(run.start + run.length);
            }
        }

        for pos in newline_boundaries(text) {
            boundaries.insert(pos);
        }

        for pos in break_boundaries(&analysis.line_breaks) {
            boundaries.insert(pos);
        }

        boundaries.into_iter().collect()
    }

    fn build_runs(
        &self,
        text: &str,
        analysis: &AnalysisResults,
        boundaries: &[u32],
        language: &str,
        options: &SegmentOptions,
    ) -> Result<Vec<TextRun>> {
        let mut runs = Vec::new();
        if boundaries.len() < 2 {
            return Ok(runs);
        }
        let unit_to_byte = unit_to_byte_map(text);

        for window in boundaries.windows(2) {
            let start = window[0];
            let end = window[1];
            if start >= end {
                continue;
            }

            let byte_start = unit_to_byte[start as usize];
            let byte_end = unit_to_byte[end as usize];
            if byte_start >= byte_end || byte_end > text.len() {
                continue;
            }

            let script = if let Some(run) = analysis.script_at(start) {
                self.script_tag(&run.analysis)?
            } else {
                "Zzzz".to_string()
            };

            let direction = if options.bidi_resolve {
                analysis
                    .bidi_at(start)
                    .map(|run| direction_from_level(run.level))
                    .unwrap_or(Direction::LeftToRight)
            } else {
                Direction::LeftToRight
            };

            runs.push(TextRun {
                text: text[byte_start..byte_end].to_string(),
                range: (byte_start, byte_end),
                script,
                language: language.to_string(),
                direction,
                font: None,
            });
        }

        Ok(runs)
    }

    fn shape_cache_key(text: &str, font: &Font, direction: Direction) -> String {
        let mut key = format!(
            "{}:{}:{}:{}:{:?}:{}",
            text,
            font.family,
            font.size,
            font.weight,
            font.style,
            direction_token(direction)
        );

        let mut variations: Vec<_> = font.variations.iter().collect();
        variations.sort_by(|a, b| a.0.cmp(b.0));
        for (tag, value) in variations {
            let _ = write!(key, ":{tag}={value}");
        }

        let mut features: Vec<_> = font.features.tags.iter().collect();
        features.sort_by(|a, b| a.0.cmp(b.0));
        for (tag, enabled) in features {
            let _ = write!(key, ":{tag}={enabled}");
        }

        key
    }

    fn configure_antialias(&self, target: &ID2D1RenderTarget, mode: AntialiasMode) -> Result<()> {
        let text_mode = match mode {
            AntialiasMode::None => D2D1_TEXT_ANTIALIAS_MODE_ALIASED,
            AntialiasMode::Grayscale => D2D1_TEXT_ANTIALIAS_MODE_GRAYSCALE,
            AntialiasMode::Subpixel => D2D1_TEXT_ANTIALIAS_MODE_CLEARTYPE,
        };
        let antialias_mode = match mode {
            AntialiasMode::None => D2D1_ANTIALIAS_MODE_ALIASED,
            _ => D2D1_ANTIALIAS_MODE_PER_PRIMITIVE,
        };
        let rendering_params = self.create_rendering_params(mode)?;
        unsafe {
            let _ = target.SetTextAntialiasMode(text_mode);
            target.SetAntialiasMode(antialias_mode);
            target
                .SetTextRenderingParams(&rendering_params)
                .map_err(|e| O4eError::render(format!("SetTextRenderingParams failed: {e}")))?;
        }
        Ok(())
    }

    fn create_rendering_params(&self, mode: AntialiasMode) -> Result<IDWriteRenderingParams> {
        let (geometry, rendering_mode, cleartype_level) = match mode {
            AntialiasMode::None => (
                DWRITE_PIXEL_GEOMETRY_FLAT,
                DWRITE_RENDERING_MODE_ALIASED,
                0.0,
            ),
            AntialiasMode::Grayscale => (
                DWRITE_PIXEL_GEOMETRY_FLAT,
                DWRITE_RENDERING_MODE_NATURAL,
                0.0,
            ),
            AntialiasMode::Subpixel => (
                DWRITE_PIXEL_GEOMETRY_RGB,
                DWRITE_RENDERING_MODE_CLEARTYPE_NATURAL_SYMMETRIC,
                1.0,
            ),
        };
        unsafe {
            self.dwrite_factory
                .CreateCustomRenderingParams(2.2, 1.0, cleartype_level, geometry, rendering_mode)
                .map_err(|e| O4eError::render(format!("CreateCustomRenderingParams failed: {e}")))
        }
    }

    fn font_metrics(font_face: &IDWriteFontFace, size: f32) -> (f32, f32) {
        let mut metrics = DWRITE_FONT_METRICS::default();
        unsafe {
            font_face.GetMetrics(&mut metrics);
        }
        let units = metrics.designUnitsPerEm.max(1) as f32;
        let ascent = metrics.ascent as f32 / units * size;
        let descent = metrics.descent as f32 / units * size;
        (ascent, descent)
    }
}

impl Backend for DirectWriteBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        if text.is_empty() {
            return Ok(Vec::new());
        }

        let language = options
            .language
            .clone()
            .unwrap_or_else(|| "und".to_string());
        let analysis = self.analyze_text(
            text.encode_utf16().collect(),
            &language,
            reading_direction(Direction::LeftToRight),
            options.bidi_resolve,
            true,
        )?;
        let boundaries = self.collect_boundaries(text, &analysis, options);
        self.build_runs(text, &analysis, &boundaries, &language, options)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        let resolved_font = run.font.as_ref().unwrap_or(font);
        let cache_key = Self::shape_cache_key(&run.text, resolved_font, run.direction);
        {
            let mut cache = self.shape_cache.write();
            if let Some(result) = cache.get(&cache_key) {
                return Ok((**result).clone());
            }
        }

        if run.text.is_empty() {
            let empty = ShapingResult {
                text: String::new(),
                glyphs: Vec::new(),
                advance: 0.0,
                bbox: o4e_core::types::BoundingBox {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: resolved_font.size.max(1.0),
                },
                font: Some(resolved_font.clone()),
                direction: run.direction,
            };
            return Ok(empty);
        }

        let locale = if run.language.is_empty() {
            "en-US"
        } else {
            &run.language
        };

        let script_analysis = self
            .analyze_text(
                run.text.encode_utf16().collect(),
                locale,
                reading_direction(run.direction),
                false,
                false,
            )?
            .script_runs
            .first()
            .map(|run| run.analysis)
            .unwrap_or(DWRITE_SCRIPT_ANALYSIS {
                script: 0,
                shapes: DWRITE_SCRIPT_SHAPES_DEFAULT,
            });

        let font_face = self.get_or_create_font_face(resolved_font)?;
        let text_wide = wide_with_null(&run.text);
        let locale_wide = wide_with_null(locale);
        let text_len = (text_wide.len() - 1) as u32;
        let mut cluster_map = vec![0u16; text_len as usize];
        let mut text_props = vec![DWRITE_SHAPING_TEXT_PROPERTIES::default(); text_len as usize];
        let max_glyphs = text_len.saturating_mul(3) / 2 + 16;
        let mut glyph_indices = vec![0u16; max_glyphs as usize];
        let mut glyph_props = vec![DWRITE_SHAPING_GLYPH_PROPERTIES::default(); max_glyphs as usize];
        let mut glyph_count = 0u32;
        let mut feature_bindings = FeatureBindings::new(font, text_len);
        let feature_ptr = feature_bindings.as_ref().map(|ctx| ctx.pointers.as_ptr());
        let feature_lengths = feature_bindings.as_ref().map(|ctx| ctx.ranges.as_ptr());
        let feature_range_count = feature_bindings
            .as_ref()
            .map(|ctx| ctx.pointers.len() as u32)
            .unwrap_or(0);

        unsafe {
            self.text_analyzer
                .GetGlyphs(
                    PCWSTR(text_wide.as_ptr()),
                    text_len,
                    &font_face,
                    BOOL::from(false),
                    BOOL::from(matches!(run.direction, Direction::RightToLeft)),
                    &script_analysis,
                    PCWSTR(locale_wide.as_ptr()),
                    None,
                    feature_ptr,
                    feature_lengths,
                    feature_range_count,
                    max_glyphs,
                    cluster_map.as_mut_ptr(),
                    text_props.as_mut_ptr(),
                    glyph_indices.as_mut_ptr(),
                    glyph_props.as_mut_ptr(),
                    &mut glyph_count,
                )
                .map_err(|e| O4eError::shaping(format!("GetGlyphs failed: {e}")))?;
        }

        glyph_indices.truncate(glyph_count as usize);
        glyph_props.truncate(glyph_count as usize);
        let mut glyph_advances = vec![0f32; glyph_count as usize];
        let mut glyph_offsets = vec![DWRITE_GLYPH_OFFSET::default(); glyph_count as usize];

        unsafe {
            self.text_analyzer
                .GetGlyphPlacements(
                    PCWSTR(text_wide.as_ptr()),
                    cluster_map.as_ptr(),
                    text_props.as_mut_ptr(),
                    text_len,
                    glyph_indices.as_ptr(),
                    glyph_props.as_ptr(),
                    glyph_count,
                    &font_face,
                    resolved_font.size,
                    BOOL::from(false),
                    BOOL::from(matches!(run.direction, Direction::RightToLeft)),
                    &script_analysis,
                    PCWSTR(locale_wide.as_ptr()),
                    None,
                    None,
                    0,
                    glyph_advances.as_mut_ptr(),
                    glyph_offsets.as_mut_ptr(),
                )
                .map_err(|e| O4eError::shaping(format!("GetGlyphPlacements failed: {e}")))?;
        }

        let unit_to_byte = unit_to_byte_map(&run.text);
        let glyph_clusters =
            glyph_clusters_from_map(&cluster_map, glyph_indices.len(), &unit_to_byte);

        let mut glyphs = Vec::with_capacity(glyph_indices.len());
        let mut advance_sum = 0.0;
        for idx in 0..glyph_indices.len() {
            let offset = glyph_offsets[idx];
            let advance = glyph_advances[idx];
            glyphs.push(Glyph {
                id: glyph_indices[idx] as u32,
                cluster: glyph_clusters.get(idx).copied().unwrap_or(0),
                x: advance_sum + offset.advanceOffset,
                y: offset.ascenderOffset,
                advance,
            });
            advance_sum += advance;
        }

        let bbox = o4e_core::utils::calculate_bbox(&glyphs);
        let shaped = ShapingResult {
            text: run.text.clone(),
            glyphs,
            advance: advance_sum,
            bbox,
            font: Some(resolved_font.clone()),
            direction: run.direction,
        };

        let shaped = Arc::new(shaped);
        {
            let mut cache = self.shape_cache.write();
            cache.push(cache_key, shaped.clone());
        }

        Ok((*shaped).clone())
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // Diagnostics removed for simplicity
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

        let font_face = self.get_or_create_font_face(font)?;
        let (ascent, descent) = Self::font_metrics(&font_face, font.size);
        let padding = options.padding as f32;
        let content_width = shaped.bbox.width.max(shaped.advance).max(1.0);
        let content_height = (ascent + descent).max(shaped.bbox.height.abs()).max(1.0);
        let width = (content_width + padding * 2.0).ceil().max(1.0) as u32;
        let height = (content_height + padding * 2.0).ceil().max(1.0) as u32;

        unsafe {
            let bitmap = self.wic_factory.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnDemand,
            )?;

            let render_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let render_target = self
                .d2d_factory
                .CreateWicBitmapRenderTarget(&bitmap, &render_props)?;
            self.configure_antialias(&render_target, options.antialias)?;

            let (text_r, text_g, text_b, text_a) =
                o4e_core::utils::parse_color(&options.color).map_err(|e| O4eError::render(e))?;

            render_target.BeginDraw();
            if options.background != "transparent" {
                let (bg_r, bg_g, bg_b, bg_a) = o4e_core::utils::parse_color(&options.background)
                    .map_err(|e| O4eError::render(e))?;
                let clear_color = D2D1_COLOR_F {
                    r: bg_r as f32 / 255.0,
                    g: bg_g as f32 / 255.0,
                    b: bg_b as f32 / 255.0,
                    a: bg_a as f32 / 255.0,
                };
                render_target.Clear(Some(&clear_color));
            } else {
                render_target.Clear(Some(&D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                }));
            }

            let brush = render_target.CreateSolidColorBrush(
                &D2D1_COLOR_F {
                    r: text_r as f32 / 255.0,
                    g: text_g as f32 / 255.0,
                    b: text_b as f32 / 255.0,
                    a: text_a as f32 / 255.0,
                },
                None,
            )?;

            let glyph_indices: Vec<u16> = shaped
                .glyphs
                .iter()
                .map(|g| g.id.min(u16::MAX as u32) as u16)
                .collect();
            let glyph_advances: Vec<f32> = shaped.glyphs.iter().map(|g| g.advance).collect();
            let mut glyph_offsets = Vec::with_capacity(shaped.glyphs.len());
            let mut pen_x = 0.0;
            for glyph in &shaped.glyphs {
                glyph_offsets.push(DWRITE_GLYPH_OFFSET {
                    advanceOffset: glyph.x - pen_x,
                    ascenderOffset: glyph.y,
                });
                pen_x += glyph.advance;
            }

            let glyph_run = DWRITE_GLYPH_RUN {
                fontFace: ManuallyDrop::new(Some(font_face.clone())),
                fontEmSize: font.size,
                glyphCount: glyph_indices.len() as u32,
                glyphIndices: glyph_indices.as_ptr(),
                glyphAdvances: glyph_advances.as_ptr(),
                glyphOffsets: glyph_offsets.as_ptr(),
                isSideways: BOOL::from(false),
                bidiLevel: if shaped.direction == Direction::RightToLeft {
                    1
                } else {
                    0
                },
            };

            let origin = D2D_POINT_2F {
                x: padding,
                y: padding + ascent,
            };
            render_target
                .DrawGlyphRun(
                    origin,
                    &glyph_run,
                    None,
                    &brush,
                    DWRITE_MEASURING_MODE_NATURAL,
                )
                .map_err(|e| O4eError::render(format!("DrawGlyphRun failed: {e}")))?;
            render_target.EndDraw(None, None)?;

            if options.format == RenderFormat::Svg {
                let svg_options = o4e_core::types::SvgOptions::default();
                let renderer = o4e_render::SvgRenderer::new(&svg_options);
                let svg = renderer.render(&shaped, &svg_options);
                return Ok(RenderOutput::Svg(svg));
            }

            let mut buffer = vec![0u8; (width * height * 4) as usize];
            let rect = WICRect {
                X: 0,
                Y: 0,
                Width: width as i32,
                Height: height as i32,
            };
            bitmap.CopyPixels(&rect, width * 4, &mut buffer)?;
            let surface = RenderSurface::from_bgra(width, height, buffer, true);
            surface.into_render_output(options.format)
        }
    }

    fn name(&self) -> &str {
        "DirectWrite"
    }

    fn clear_cache(&self) {
        self.cache.clear();
        self.font_cache.write().clear();
        self.shape_cache.write().clear();
    }
}

impl Default for DirectWriteBackend {
    fn default() -> Self {
        Self::new().expect("Failed to initialize DirectWrite backend")
    }
}

fn unit_to_byte_map(text: &str) -> Vec<usize> {
    let total_units = text.encode_utf16().count();
    let mut map = vec![0; total_units + 1];
    let mut unit_index = 0;
    for (byte_idx, ch) in text.char_indices() {
        let units = ch.len_utf16();
        for offset in 0..units {
            map[unit_index + offset] = byte_idx;
        }
        unit_index += units;
    }
    map[total_units] = text.len();
    map
}

fn newline_boundaries(text: &str) -> Vec<u32> {
    let mut positions = Vec::new();
    let mut unit_index = 0u32;
    for ch in text.chars() {
        let units = ch.len_utf16() as u32;
        if ch == '\n' || ch == '\r' {
            positions.push(unit_index);
            positions.push(unit_index + units);
        }
        unit_index += units;
    }
    positions
}

fn break_boundaries(breaks: &[DWRITE_LINE_BREAKPOINT]) -> Vec<u32> {
    breaks
        .iter()
        .enumerate()
        .filter_map(|(idx, bp)| {
            if is_must_break(*bp) {
                Some(idx as u32 + 1)
            } else {
                None
            }
        })
        .collect()
}

fn is_must_break(bp: DWRITE_LINE_BREAKPOINT) -> bool {
    ((bp._bitfield >> 2) & 0b11) == DWRITE_BREAK_CONDITION_MUST_BREAK.0 as u8
}

fn direction_from_level(level: u8) -> Direction {
    if level % 2 == 1 {
        Direction::RightToLeft
    } else {
        Direction::LeftToRight
    }
}

fn direction_token(direction: Direction) -> &'static str {
    match direction {
        Direction::RightToLeft => "rtl",
        Direction::Auto => "auto",
        _ => "ltr",
    }
}

fn reading_direction(direction: Direction) -> DWRITE_READING_DIRECTION {
    match direction {
        Direction::RightToLeft => DWRITE_READING_DIRECTION_RIGHT_TO_LEFT,
        _ => DWRITE_READING_DIRECTION_LEFT_TO_RIGHT,
    }
}

fn wide_with_null(value: &str) -> Vec<u16> {
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    wide.push(0);
    wide
}

fn glyph_clusters_from_map(
    cluster_map: &[u16],
    glyph_count: usize,
    unit_to_byte: &[usize],
) -> Vec<u32> {
    if glyph_count == 0 {
        return Vec::new();
    }

    let mut clusters = vec![0u32; glyph_count];
    if cluster_map.is_empty() {
        return clusters;
    }

    let mut text_index = 0usize;
    while text_index < cluster_map.len() {
        let glyph_index = cluster_map[text_index] as usize;
        if glyph_index >= glyph_count {
            text_index += 1;
            continue;
        }
        let byte_offset = unit_to_byte[text_index] as u32;
        let mut next_text = text_index + 1;
        while next_text < cluster_map.len() && cluster_map[next_text] as usize == glyph_index {
            next_text += 1;
        }
        let next_glyph = if next_text < cluster_map.len() {
            cluster_map[next_text] as usize
        } else {
            glyph_count
        };
        for glyph in glyph_index..next_glyph.min(glyph_count) {
            clusters[glyph] = byte_offset;
        }
        text_index = next_text;
    }

    let mut last = clusters[0];
    for cluster in clusters.iter_mut() {
        if *cluster == 0 {
            *cluster = last;
        } else {
            last = *cluster;
        }
    }
    clusters
}

fn iso_script_tag(code: u32) -> String {
    let bytes = [
        ((code >> 24) & 0xFF) as u8,
        ((code >> 16) & 0xFF) as u8,
        ((code >> 8) & 0xFF) as u8,
        (code & 0xFF) as u8,
    ];
    let mut tag = String::new();
    for byte in bytes {
        if byte != 0 {
            tag.push(byte as char);
        }
    }
    if tag.is_empty() {
        "Zzzz".to_string()
    } else {
        tag
    }
}

fn variation_overrides(font: &Font) -> HashMap<u32, f32> {
    let mut overrides = HashMap::new();
    for (tag, value) in &font.variations {
        if let Some(raw) = ot_tag(tag) {
            overrides.insert(raw, *value);
        }
    }
    overrides
}

fn ot_tag(tag: &str) -> Option<u32> {
    let bytes = tag.as_bytes();
    if bytes.len() != 4 {
        return None;
    }
    let value = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    #[test]
    fn segment_mixed_scripts_produces_runs() {
        let backend = DirectWriteBackend::new().expect("backend");
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        options.bidi_resolve = true;
        options.language = Some("en-US".to_string());

        let runs = backend
            .segment("Hello مرحبا", &options)
            .expect("segment result");
        assert!(
            runs.len() >= 2,
            "expected at least two runs, got {}",
            runs.len()
        );
        assert!(
            runs.iter()
                .any(|run| run.direction == Direction::LeftToRight),
            "missing LTR run"
        );
        assert!(
            runs.iter()
                .any(|run| run.direction == Direction::RightToLeft),
            "missing RTL run"
        );
        assert!(
            runs.iter().all(|run| !run.script.is_empty()),
            "scripts should be populated"
        );
    }

    #[test]
    fn shape_generates_real_glyphs() {
        let backend = DirectWriteBackend::new().expect("backend");
        let mut options = SegmentOptions::default();
        options.script_itemize = true;
        let runs = backend.segment("DirectWrite", &options).expect("segment");
        let font = Font::new("Segoe UI", 28.0);
        let shaped = backend.shape(&runs[0], &font).expect("shape");
        assert!(!shaped.glyphs.is_empty(), "expected glyphs for sample text");
        assert!(shaped.advance > 0.0, "advance should be positive");
        assert!(shaped.bbox.width > 0.0, "bbox width should be positive");
        assert!(shaped.font.is_some(), "font information should be attached");
        assert_eq!(shaped.direction, runs[0].direction);
    }

    #[test]
    fn render_bitmap_differs_for_antialias_modes() {
        let backend = DirectWriteBackend::new().expect("backend");
        let runs = backend
            .segment("ClearType sample", &SegmentOptions::default())
            .expect("segment");
        let font = Font::new("Segoe UI", 36.0);
        let shaped = backend.shape(&runs[0], &font).expect("shape");
        let mut options = RenderOptions::default();
        options.format = RenderFormat::Raw;

        options.antialias = AntialiasMode::Subpixel;
        let cleartype = backend.render(&shaped, &options).expect("render cleartype");
        options.antialias = AntialiasMode::Grayscale;
        let grayscale = backend.render(&shaped, &options).expect("render grayscale");
        assert_ne!(
            bitmap_hash(&cleartype),
            bitmap_hash(&grayscale),
            "expected grayscale vs ClearType hashes to differ"
        );
    }

    #[test]
    fn render_bitmap_differs_when_liga_disabled() {
        let backend = DirectWriteBackend::new().expect("backend");
        let runs = backend
            .segment("office", &SegmentOptions::default())
            .expect("segment");
        let mut enabled_font = Font::new("Segoe UI", 48.0);
        let mut options = RenderOptions::default();
        options.format = RenderFormat::Raw;

        let shaped_enabled = backend.shape(&runs[0], &enabled_font).expect("shape");
        let ligature_hash = bitmap_hash(
            &backend
                .render(&shaped_enabled, &options)
                .expect("render liga on"),
        );

        enabled_font.features.tags.insert("liga".to_string(), false);
        let shaped_disabled = backend.shape(&runs[0], &enabled_font).expect("shape");
        let no_ligature_hash = bitmap_hash(
            &backend
                .render(&shaped_disabled, &options)
                .expect("render liga off"),
        );
        assert_ne!(
            ligature_hash, no_ligature_hash,
            "expected ligature toggle to change bitmap output"
        );
    }

    #[test]
    fn render_bitmap_differs_for_variable_font_weights() {
        let backend = DirectWriteBackend::new().expect("backend");
        let runs = backend
            .segment("Variable", &SegmentOptions::default())
            .expect("segment");
        let mut options = RenderOptions::default();
        options.format = RenderFormat::Raw;

        let mut light_font = Font::new("Bahnschrift", 48.0);
        light_font.variations.insert("wght".to_string(), 200.0);
        let shaped_light = backend.shape(&runs[0], &light_font).expect("shape");
        let light_hash = bitmap_hash(
            &backend
                .render(&shaped_light, &options)
                .expect("render light"),
        );

        let mut heavy_font = Font::new("Bahnschrift", 48.0);
        heavy_font.variations.insert("wght".to_string(), 850.0);
        let shaped_heavy = backend.shape(&runs[0], &heavy_font).expect("shape");
        let heavy_hash = bitmap_hash(
            &backend
                .render(&shaped_heavy, &options)
                .expect("render heavy"),
        );
        assert_ne!(
            light_hash, heavy_hash,
            "expected variable font axes to influence rendering"
        );
    }

    #[test]
    fn clear_cache_removes_cached_font_faces() {
        let backend = DirectWriteBackend::new().expect("backend");
        let runs = backend
            .segment("Cache warmup", &SegmentOptions::default())
            .expect("segment");
        let font = Font::new("Segoe UI", 30.0);
        let shaped = backend.shape(&runs[0], &font).expect("shape");
        backend
            .render(&shaped, &RenderOptions::default())
            .expect("render");

        assert!(
            backend.font_cache.read().len() > 0,
            "font cache should contain entries before clearing"
        );
        backend.clear_cache();
        assert_eq!(
            backend.font_cache.read().len(),
            0,
            "font cache should be empty after clear_cache"
        );
    }

    fn bitmap_hash(output: &RenderOutput) -> u64 {
        let bytes = match output {
            RenderOutput::Bitmap(bitmap) => &bitmap.data,
            _ => panic!("expected bitmap output"),
        };
        let mut hasher = DefaultHasher::new();
        bytes.hash(&mut hasher);
        hasher.finish()
    }
    #[test]
    fn backend_creation_reports_name() {
        let backend = DirectWriteBackend::new();
        assert!(backend.is_ok());
        if let Ok(backend) = backend {
            assert_eq!(backend.name(), "DirectWrite");
        }
    }

    #[test]
    fn simple_segmentation_round_trip() {
        if let Ok(backend) = DirectWriteBackend::new() {
            let options = SegmentOptions::default();
            let runs = backend.segment("Hello World", &options).unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].text, "Hello World");
        }
    }
}
