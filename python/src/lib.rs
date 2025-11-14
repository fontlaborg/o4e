// this_file: python/src/lib.rs

//! Python bindings for the o4e text rendering engine.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::Arc;

use o4e_core::{
    types::{
        AntialiasMode, BoundingBox, Direction, Features, FontSource, FontStyle, HintingMode,
        RenderFormat,
    },
    utils::combine_shaped_results,
    Backend, Font as CoreFont, Glyph as CoreGlyph, RenderOptions as CoreRenderOptions,
    RenderOutput, SegmentOptions, ShapingResult as CoreShapingResult, TextRun,
};
use pyo3::PyAny;
use pyo3::{
    exceptions::{PyRuntimeError, PyValueError},
    prelude::*,
    types::{
        PyAnyMethods, PyBytes, PyDict, PyDictMethods, PyList, PyListMethods, PyString, PyTuple,
    },
    IntoPy,
};
use pyo3::{Bound, PyRef, PyType};

#[cfg(all(target_os = "macos", feature = "mac"))]
use o4e_mac::CoreTextBackend;

#[cfg(all(target_os = "windows", feature = "windows"))]
use o4e_win::DirectWriteBackend;

#[cfg(feature = "icu")]
use o4e_icu_hb::HarfBuzzBackend;

/// Python-facing font specification.
#[pyclass]
#[derive(Clone)]
struct Font {
    #[pyo3(get)]
    family: String,
    #[pyo3(get)]
    size: f32,
    #[pyo3(get)]
    weight: u16,
    #[pyo3(get)]
    style: String,
    source: FontSource,
    variations: HashMap<String, f32>,
    features: HashMap<String, bool>,
}

impl Font {
    fn to_core_font(&self) -> CoreFont {
        let font_style = match self.style.as_str() {
            "italic" => FontStyle::Italic,
            "oblique" => FontStyle::Oblique,
            _ => FontStyle::Normal,
        };

        CoreFont {
            family: self.family.clone(),
            size: self.size,
            weight: self.weight,
            style: font_style,
            variations: self.variations.clone(),
            features: Features {
                tags: self.features.clone(),
            },
            source: self.source.clone(),
        }
    }
}

#[pymethods]
impl Font {
    #[new]
    #[pyo3(signature = (family, size=None, weight=None, style=None, variations=None, features=None))]
    fn new(
        family: String,
        size: Option<f32>,
        weight: Option<u16>,
        style: Option<String>,
        variations: Option<HashMap<String, f32>>,
        features: Option<HashMap<String, bool>>,
    ) -> Self {
        Self {
            family: family.clone(),
            size: size.unwrap_or(16.0),
            weight: weight.unwrap_or(400),
            style: style.unwrap_or_else(|| "normal".to_string()),
            source: FontSource::Family(family),
            variations: variations.unwrap_or_default(),
            features: features.unwrap_or_default(),
        }
    }

    #[classmethod]
    #[pyo3(signature = (path, size=None, weight=None, style=None, variations=None, features=None))]
    fn from_path(
        _cls: &Bound<'_, PyType>,
        path: String,
        size: Option<f32>,
        weight: Option<u16>,
        style: Option<String>,
        variations: Option<HashMap<String, f32>>,
        features: Option<HashMap<String, bool>>,
    ) -> Self {
        Self {
            family: path.clone(),
            size: size.unwrap_or(16.0),
            weight: weight.unwrap_or(400),
            style: style.unwrap_or_else(|| "normal".to_string()),
            source: FontSource::Path(path),
            variations: variations.unwrap_or_default(),
            features: features.unwrap_or_default(),
        }
    }

    #[classmethod]
    #[pyo3(
        signature = (name, data, size=None, weight=None, style=None, variations=None, features=None)
    )]
    fn from_bytes(
        _cls: &Bound<'_, PyType>,
        name: String,
        data: &Bound<'_, PyAny>,
        size: Option<f32>,
        weight: Option<u16>,
        style: Option<String>,
        variations: Option<HashMap<String, f32>>,
        features: Option<HashMap<String, bool>>,
    ) -> PyResult<Self> {
        let bytes: Vec<u8> = data.extract()?;
        Ok(Self {
            family: name.clone(),
            size: size.unwrap_or(16.0),
            weight: weight.unwrap_or(400),
            style: style.unwrap_or_else(|| "normal".to_string()),
            source: FontSource::Bytes {
                name,
                data: Arc::from(bytes.into_boxed_slice()),
            },
            variations: variations.unwrap_or_default(),
            features: features.unwrap_or_default(),
        })
    }
}

/// Glyph information returned to Python.
#[pyclass]
#[derive(Clone)]
struct Glyph {
    #[pyo3(get)]
    id: u32,
    #[pyo3(get)]
    cluster: u32,
    #[pyo3(get)]
    x: f32,
    #[pyo3(get)]
    y: f32,
    #[pyo3(get)]
    advance: f32,
}

impl Glyph {
    fn from_core(glyph: &CoreGlyph) -> Self {
        Self {
            id: glyph.id,
            cluster: glyph.cluster,
            x: glyph.x,
            y: glyph.y,
            advance: glyph.advance,
        }
    }
}

/// Shaping result returned to Python.
#[pyclass]
#[derive(Clone)]
struct ShapingResult {
    #[pyo3(get)]
    text: String,
    #[pyo3(get)]
    glyphs: Vec<Glyph>,
    #[pyo3(get)]
    advance: f32,
    #[pyo3(get)]
    width: f32,
    #[pyo3(get)]
    height: f32,
}

impl ShapingResult {
    fn from_core(result: &CoreShapingResult) -> Self {
        Self {
            text: result.text.clone(),
            glyphs: result.glyphs.iter().map(Glyph::from_core).collect(),
            advance: result.advance,
            width: result.bbox.width,
            height: result.bbox.height,
        }
    }
}

/// Runtime overrides applied to shaped runs.
#[derive(Default, Clone)]
struct ShapeOverrides {
    direction: Option<Direction>,
    language: Option<String>,
    script: Option<String>,
}

impl ShapeOverrides {
    fn apply(&self, run: &mut TextRun) {
        if let Some(direction) = self.direction {
            run.direction = direction;
        }
        if let Some(language) = &self.language {
            run.language = language.clone();
        }
        if let Some(script) = &self.script {
            run.script = script.clone();
        }
    }
}

struct RenderConfig {
    render: CoreRenderOptions,
    segment: SegmentOptions,
    overrides: ShapeOverrides,
}

/// Main Python-facing renderer class.
#[pyclass]
struct TextRenderer {
    backend: Box<dyn Backend>,
}

#[pymethods]
impl TextRenderer {
    #[new]
    #[pyo3(signature = (backend=None))]
    fn new(backend: Option<String>) -> PyResult<Self> {
        let backend_name = backend.as_deref().unwrap_or("auto");
        let backend: Box<dyn Backend> = match backend_name {
            #[cfg(all(target_os = "macos", feature = "mac"))]
            "coretext" => Box::new(CoreTextBackend::new()),

            #[cfg(all(target_os = "windows", feature = "windows"))]
            "directwrite" => Box::new(
                DirectWriteBackend::new()
                    .map_err(|e| runtime_err("Failed to initialize DirectWrite backend", e))?,
            ),

            #[cfg(feature = "icu")]
            "harfbuzz" => Box::new(HarfBuzzBackend::new()),

            "auto" => Self::auto_backend()?,

            #[cfg(not(feature = "icu"))]
            "harfbuzz" => {
                return Err(PyRuntimeError::new_err(
                    "HarfBuzz backend is not available in this build",
                ))
            }

            _ => {
                return Err(PyValueError::new_err(format!(
                    "Unknown backend: {backend_name}"
                )))
            }
        };

        Ok(Self { backend })
    }

    /// Check if o4e is available.
    #[staticmethod]
    fn is_available() -> bool {
        true
    }

    /// Get version string.
    #[staticmethod]
    fn version() -> String {
        env!("CARGO_PKG_VERSION").to_string()
    }

    /// Render text to the requested format.
    #[pyo3(signature = (text, font, format=None, render_options=None))]
    fn render<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        font: &Font,
        format: Option<&str>,
        render_options: Option<Bound<'py, PyDict>>,
    ) -> PyResult<PyObject> {
        let render_format = parse_render_format(format)?;
        let render_options_ref = render_options.as_ref();
        self.render_internal(py, text, font, render_format, render_options_ref)
    }

    /// Shape text without rendering.
    #[pyo3(signature = (text, font, shape_options=None))]
    fn shape<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        font: &Font,
        shape_options: Option<Bound<'py, PyDict>>,
    ) -> PyResult<Py<ShapingResult>> {
        let core_font = font.to_core_font();
        let shape_options_ref = shape_options.as_ref();
        let (segment_options, overrides) = build_shape_config(shape_options_ref)?;
        let mut runs = self
            .backend
            .segment(text, &segment_options)
            .map_err(|e| runtime_err("Segmentation error", e))?;

        let shaped = if runs.is_empty() {
            empty_shaping_result(&core_font)
        } else {
            self.shape_runs(&mut runs, &core_font, &overrides)?
        };

        Py::new(py, ShapingResult::from_core(&shaped))
    }

    /// Render a batch of items (currently sequential).
    #[pyo3(signature = (items, format=None, max_workers=None))]
    fn render_batch<'py>(
        &self,
        py: Python<'py>,
        items: &Bound<'py, PyAny>,
        format: Option<&str>,
        max_workers: Option<usize>,
    ) -> PyResult<PyObject> {
        let _ = max_workers;
        let list = items.downcast::<PyList>()?;
        let render_format = parse_render_format(format)?;
        let mut results = Vec::with_capacity(list.len());

        for item in list.iter() {
            let dict = item.downcast::<PyDict>()?;
            let text_value = dict
                .get_item("text")?
                .ok_or_else(|| PyValueError::new_err("Batch item missing 'text' key"))?;
            let text: String = text_value.extract()?;

            let font_obj = dict
                .get_item("font")?
                .ok_or_else(|| PyValueError::new_err("Batch item missing 'font' key"))?;
            let font_py_ref: PyRef<'py, Font> = font_obj.extract()?;
            let font_ref: &Font = &*font_py_ref;

            let mut options_dict: Bound<'py, PyDict> = PyDict::new_bound(py);
            for (key, value) in dict.iter() {
                if let Ok(name) = key.extract::<&str>() {
                    if name == "text" || name == "font" {
                        continue;
                    }
                }
                options_dict.set_item(key, value)?;
            }

            let result = if options_dict.is_empty() {
                self.render_internal(py, &text, font_ref, render_format, None)?
            } else {
                self.render_internal(py, &text, font_ref, render_format, Some(&options_dict))?
            };
            results.push(result);
        }

        let py_list: Bound<'py, PyList> = PyList::new_bound(py, results);
        Ok(py_list.into_any().into_py(py))
    }

    /// Clear backend caches.
    fn clear_cache(&self) {
        self.backend.clear_cache();
    }

    fn __repr__(&self) -> PyResult<String> {
        Ok(format!("TextRenderer(backend=\"{}\")", self.backend.name()))
    }
}

impl TextRenderer {
    fn auto_backend() -> PyResult<Box<dyn Backend>> {
        #[cfg(all(target_os = "macos", feature = "mac"))]
        {
            return Ok(Box::new(CoreTextBackend::new()));
        }

        #[cfg(all(target_os = "windows", feature = "windows"))]
        {
            return Ok(Box::new(DirectWriteBackend::new().map_err(|e| {
                runtime_err("Failed to initialize DirectWrite backend", e)
            })?));
        }

        #[cfg(feature = "icu")]
        {
            return Ok(Box::new(HarfBuzzBackend::new()));
        }

        Err(PyRuntimeError::new_err(
            "No backend available for this platform",
        ))
    }

    fn render_internal<'py>(
        &self,
        py: Python<'py>,
        text: &str,
        font: &Font,
        format: RenderFormat,
        render_options: Option<&Bound<'py, PyDict>>,
    ) -> PyResult<PyObject> {
        let core_font = font.to_core_font();
        let config = build_render_config(render_options, format)?;
        let mut runs = self
            .backend
            .segment(text, &config.segment)
            .map_err(|e| runtime_err("Segmentation error", e))?;

        let shaped = if runs.is_empty() {
            empty_shaping_result(&core_font)
        } else {
            self.shape_runs(&mut runs, &core_font, &config.overrides)?
        };

        let output = self
            .backend
            .render(&shaped, &config.render)
            .map_err(|e| runtime_err("Render error", e))?;

        self.output_to_py(py, output)
    }

    fn shape_runs(
        &self,
        runs: &mut [TextRun],
        font: &CoreFont,
        overrides: &ShapeOverrides,
    ) -> PyResult<CoreShapingResult> {
        let mut shaped_segments = Vec::with_capacity(runs.len());
        for run in runs {
            overrides.apply(run);
            let shaped = self
                .backend
                .shape(run, font)
                .map_err(|e| runtime_err("Shaping error", e))?;
            shaped_segments.push(shaped);
        }
        Ok(combine_shaped_results(shaped_segments))
    }

    fn output_to_py<'py>(&self, py: Python<'py>, output: RenderOutput) -> PyResult<PyObject> {
        Ok(match output {
            RenderOutput::Bitmap(bitmap) => {
                let bytes = PyBytes::new_bound(py, &bitmap.data);
                let tuple = PyTuple::new_bound(
                    py,
                    &[
                        bytes.into_any().into_py(py),
                        bitmap.width.into_py(py),
                        bitmap.height.into_py(py),
                    ],
                );
                tuple.into_any().into_py(py)
            }
            RenderOutput::Png(data) | RenderOutput::Raw(data) => {
                PyBytes::new_bound(py, &data).into_any().into_py(py)
            }
            RenderOutput::Svg(svg) => PyString::new_bound(py, &svg).into_any().into_py(py),
        })
    }
}

/// Python module definition.
#[pymodule]
fn o4e(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TextRenderer>()?;
    m.add_class::<Font>()?;
    m.add_class::<Glyph>()?;
    m.add_class::<ShapingResult>()?;
    m.add_function(wrap_pyfunction!(get_version, m)?)?;
    Ok(())
}

/// Get the version of o4e.
#[pyfunction]
fn get_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

fn build_render_config(
    options: Option<&Bound<'_, PyDict>>,
    format: RenderFormat,
) -> PyResult<RenderConfig> {
    let mut render = CoreRenderOptions::default();
    render.format = format;
    let mut segment = SegmentOptions::default();
    let mut overrides = ShapeOverrides::default();

    if let Some(opts) = options {
        if let Some(color) = opts.get_item("color")? {
            render.color = color.extract::<String>()?;
        }
        if let Some(background) = opts.get_item("background")? {
            render.background = background.extract::<String>()?;
        }
        if let Some(padding) = opts.get_item("padding")? {
            let value: i32 = padding.extract()?;
            if value < 0 {
                return Err(PyValueError::new_err("padding must be >= 0"));
            }
            render.padding = value as u32;
        }
        if let Some(dpi) = opts.get_item("dpi")? {
            render.dpi = dpi.extract::<f32>()?;
        }
        if let Some(aa) = opts.get_item("antialias")? {
            render.antialias = parse_antialias(&aa)?;
        }
        if let Some(hinting) = opts.get_item("hinting")? {
            render.hinting = parse_hinting(&hinting)?;
        }
        if let Some(direction) = opts.get_item("direction")? {
            overrides.direction = Some(parse_direction(&direction)?);
        }
        if let Some(language) = opts.get_item("language")? {
            let lang = language.extract::<String>()?;
            segment.language = Some(lang.clone());
            overrides.language = Some(lang);
        }
        if let Some(script) = opts.get_item("script")? {
            overrides.script = Some(script.extract::<String>()?);
        }
        if let Some(value) = opts.get_item("font_fallback")? {
            segment.font_fallback = value.extract::<bool>()?;
        }
        if let Some(value) = opts.get_item("script_itemize")? {
            segment.script_itemize = value.extract::<bool>()?;
        }
        if let Some(value) = opts.get_item("bidi_resolve")? {
            segment.bidi_resolve = value.extract::<bool>()?;
        }
    }

    Ok(RenderConfig {
        render,
        segment,
        overrides,
    })
}

fn build_shape_config(
    options: Option<&Bound<'_, PyDict>>,
) -> PyResult<(SegmentOptions, ShapeOverrides)> {
    let config = build_render_config(options, RenderFormat::Raw)?;
    Ok((config.segment, config.overrides))
}

fn parse_render_format(value: Option<&str>) -> PyResult<RenderFormat> {
    Ok(match value.map(|s| s.to_lowercase()) {
        Some(ref s) if s == "png" => RenderFormat::Png,
        Some(ref s) if s == "svg" => RenderFormat::Svg,
        Some(ref s) if s == "raw" => RenderFormat::Raw,
        None => RenderFormat::Raw,
        Some(other) => {
            return Err(PyValueError::new_err(format!(
                "Unsupported render format: {other}"
            )))
        }
    })
}

fn parse_direction(value: &Bound<'_, PyAny>) -> PyResult<Direction> {
    let direction = value.extract::<String>()?.to_lowercase();
    match direction.as_str() {
        "ltr" | "left-to-right" => Ok(Direction::LeftToRight),
        "rtl" | "right-to-left" => Ok(Direction::RightToLeft),
        "auto" => Ok(Direction::Auto),
        _ => Err(PyValueError::new_err(format!(
            "Invalid direction: {direction}"
        ))),
    }
}

fn parse_antialias(value: &Bound<'_, PyAny>) -> PyResult<AntialiasMode> {
    if let Ok(enabled) = value.extract::<bool>() {
        return Ok(if enabled {
            AntialiasMode::Subpixel
        } else {
            AntialiasMode::None
        });
    }

    match value.extract::<String>()?.to_lowercase().as_str() {
        "none" => Ok(AntialiasMode::None),
        "grayscale" => Ok(AntialiasMode::Grayscale),
        "subpixel" => Ok(AntialiasMode::Subpixel),
        other => Err(PyValueError::new_err(format!(
            "Invalid antialias mode: {other}"
        ))),
    }
}

fn parse_hinting(value: &Bound<'_, PyAny>) -> PyResult<HintingMode> {
    if let Ok(enabled) = value.extract::<bool>() {
        return Ok(if enabled {
            HintingMode::Slight
        } else {
            HintingMode::None
        });
    }

    match value.extract::<String>()?.to_lowercase().as_str() {
        "none" => Ok(HintingMode::None),
        "slight" => Ok(HintingMode::Slight),
        "full" => Ok(HintingMode::Full),
        other => Err(PyValueError::new_err(format!(
            "Invalid hinting mode: {other}"
        ))),
    }
}

fn empty_shaping_result(font: &CoreFont) -> CoreShapingResult {
    CoreShapingResult {
        text: String::new(),
        glyphs: Vec::new(),
        advance: 0.0,
        bbox: BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: font.size.max(1.0),
        },
        font: Some(font.clone()),
        direction: Direction::LeftToRight,
    }
}

fn runtime_err<E: Display>(msg: &str, err: E) -> PyErr {
    PyRuntimeError::new_err(format!("{msg}: {err}"))
}
