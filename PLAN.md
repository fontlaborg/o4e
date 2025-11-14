---
this_file: PLAN.md
---

# o4e Implementation Plan - Rapid Development Roadmap

## Executive Summary

Transform o4e from single-backend renderer to multi-backend metapackage with native platform integration. Target: MVP in 2 weeks, production in 8 weeks.

## Week 1: Foundation & First Backend

### Day 1-2: Workspace Setup

#### Step 1: Create Cargo Workspace Structure
```bash
# Root directory structure
mkdir -p backends/{o4e-core,o4e-mac,o4e-win,o4e-icu-hb,o4e-pure}
mkdir -p crates/{o4e-api,o4e-unicode,o4e-shaping,o4e-render}
mkdir -p python/{src,o4e,tests}
mkdir -p examples/{basic,advanced,benchmarks}
```

#### Step 2: Root Cargo.toml
```toml
[workspace]
resolver = "2"
members = [
    "backends/*",
    "crates/*",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Font Laboratory <team@fontlab.org>"]
license = "MIT OR Apache-2.0"
repository = "https://github.com/fontlaborg/o4e"

[workspace.dependencies]
# Core dependencies
thiserror = "1.0"
anyhow = "1.0"
log = "0.4"
env_logger = "0.11"

# Performance
rayon = "1.10"
lru = "0.12"
memmap2 = "0.9"
parking_lot = "0.12"

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# Font/Text
harfbuzz_rs = "2.0"
icu_segmenter = "1.5"
icu_locid = "1.5"
ttf-parser = "0.24"

# Platform-specific
[target.'cfg(target_os = "macos")'.workspace.dependencies]
objc2 = "0.5"
objc2-foundation = "0.2"
core-foundation = "0.10"
core-graphics = "0.24"
core-text = "20.1"

[target.'cfg(windows)'.workspace.dependencies]
windows = { version = "0.58", features = [
    "Win32_Graphics_DirectWrite",
    "Win32_Graphics_Direct2D",
    "Win32_System_Com",
]}
```

#### Step 3: Core Traits (backends/o4e-core/src/lib.rs)
```rust
pub mod traits;
pub mod types;
pub mod cache;
pub mod error;
pub mod utils;

pub use traits::{Backend, TextSegmenter, FontShaper, GlyphRenderer};
pub use types::{Font, TextRun, ShapingResult, RenderOutput};
pub use cache::FontCache;
pub use error::O4eError;
```

#### Step 4: Core Traits Definition (backends/o4e-core/src/traits.rs)
```rust
use crate::types::*;

/// Main backend trait - all backends must implement this
pub trait Backend: Send + Sync {
    /// Segment text into runs for rendering
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>>;

    /// Shape a text run into glyphs
    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult>;

    /// Render shaped glyphs to output
    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput>;

    /// Backend name for identification
    fn name(&self) -> &str;
}

/// Text segmentation trait
pub trait TextSegmenter {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>>;
}

/// Font shaping trait
pub trait FontShaper {
    fn shape(&self, text: &str, font: &Font, features: &Features) -> Result<ShapingResult>;
}

/// Glyph rendering trait
pub trait GlyphRenderer {
    fn render_to_bitmap(&self, glyphs: &[Glyph], options: &RenderOptions) -> Result<Bitmap>;
    fn render_to_svg(&self, glyphs: &[Glyph], options: &SvgOptions) -> Result<String>;
}
```

### Day 3-4: CoreText Backend (macOS)

#### Step 5: o4e-mac Cargo.toml
```toml
[package]
name = "o4e-mac"
version.workspace = true
edition.workspace = true

[dependencies]
o4e-core = { path = "../o4e-core" }
objc2.workspace = true
objc2-foundation.workspace = true
core-foundation.workspace = true
core-graphics.workspace = true
core-text.workspace = true
thiserror.workspace = true
log.workspace = true
```

#### Step 6: CoreText Implementation (backends/o4e-mac/src/lib.rs)
```rust
use o4e_core::{Backend, TextSegmenter, FontShaper, GlyphRenderer};
use objc2::rc::Id;
use objc2_foundation::{NSString, NSAttributedString};
use core_text::{CTFont, CTLine, CTRun};
use core_graphics::{CGContext, CGImage};

pub struct CoreTextBackend {
    cache: FontCache,
}

impl CoreTextBackend {
    pub fn new() -> Self {
        Self {
            cache: FontCache::new(512),
        }
    }

    fn create_attributed_string(&self, text: &str, font: &Font) -> NSAttributedString {
        // 1. Create NSString from text
        // 2. Create CTFont from font spec
        // 3. Create attributes dictionary
        // 4. Return NSAttributedString
    }

    fn extract_glyphs(&self, line: &CTLine) -> Vec<Glyph> {
        // 1. Get runs from line
        // 2. For each run, extract glyphs
        // 3. Get positions and advances
        // 4. Return glyph vector
    }
}

impl Backend for CoreTextBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        // Use Core Foundation's CFStringTokenizer
        // Split by script, direction, language
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // 1. Create attributed string
        // 2. Create CTLine
        // 3. Extract glyphs
        // 4. Return ShapingResult
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // 1. Create CGContext
        // 2. Draw glyphs
        // 3. Extract bitmap or generate SVG
        // 4. Return RenderOutput
    }

    fn name(&self) -> &str {
        "CoreText"
    }
}
```

#### Verification
- [x] Render Latin sample text via CoreText (`Helvetica`) to confirm glyph replay and bitmap output.
- [x] Render Arabic sample (`Geeza Pro`) with bidi enabled to verify RTL runs render without panics.
- [x] Render CJK sample (`PingFang SC`) to ensure Han scripts survive the CoreText pipeline.

### Day 5: Python Bindings Foundation

#### Step 7: PyO3 Setup (python/src/lib.rs)
```rust
use pyo3::prelude::*;
use o4e_core::{Backend, Font, RenderOptions};

#[cfg(target_os = "macos")]
use o4e_mac::CoreTextBackend;

#[cfg(target_os = "windows")]
use o4e_win::DirectWriteBackend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use o4e_icu_hb::HarfBuzzBackend;

/// Main Python-facing renderer class
#[pyclass]
struct TextRenderer {
    backend: Box<dyn Backend>,
}

#[pymethods]
impl TextRenderer {
    #[new]
    fn new(backend: Option<String>) -> PyResult<Self> {
        let backend: Box<dyn Backend> = match backend.as_deref() {
            #[cfg(target_os = "macos")]
            Some("coretext") | None => Box::new(CoreTextBackend::new()),

            #[cfg(target_os = "windows")]
            Some("directwrite") | None => Box::new(DirectWriteBackend::new()),

            Some("harfbuzz") => Box::new(HarfBuzzBackend::new()),

            _ => return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
                format!("Unknown backend: {}", backend.unwrap())
            )),
        };

        Ok(Self { backend })
    }

    fn render(&self, text: &str, font: PyFont, output_format: &str) -> PyResult<PyObject> {
        // Convert PyFont to Font
        // Call backend.render()
        // Convert result to Python object
    }

    fn shape(&self, text: &str, font: PyFont) -> PyResult<ShapingResult> {
        // Segment text
        // Shape each run
        // Return combined result
    }
}

#[pymodule]
fn o4e(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<TextRenderer>()?;
    m.add_class::<PyFont>()?;
    m.add_class::<ShapingResult>()?;
    Ok(())
}
```

**Status 2024-11-13:** Implemented the `render`, `shape`, and `render_batch` methods plus the `Glyph`/`ShapingResult` Python classes so the high-level Python API can drive the Rust backend without mocks.

## Week 2: Cross-Platform & Features

### Day 6-7: ICU+HarfBuzz Backend

#### Step 8: o4e-icu-hb Implementation
```rust
use o4e_core::*;
use harfbuzz_rs::{Face, Font as HbFont, UnicodeBuffer, GlyphBuffer};
use icu_segmenter::{GraphemeClusterSegmenter, LineSegmenter, WordSegmenter};
use freetype::{Library, Face as FtFace};

pub struct HarfBuzzBackend {
    hb_cache: LruCache<String, Arc<HbFont>>,
    ft_library: Library,
}

impl Backend for HarfBuzzBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        // Use ICU segmenter for:
        // 1. Grapheme cluster boundaries
        // 2. Word boundaries
        // 3. Line break opportunities
        // 4. Script itemization
        // 5. Bidi resolution
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // 1. Load font with HarfBuzz
        // 2. Create buffer from text
        // 3. Set buffer properties (direction, script, language)
        // 4. Shape with features
        // 5. Extract glyph info and positions
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // 1. Load font with FreeType
        // 2. Set size and hinting
        // 3. Render each glyph to bitmap
        // 4. Composite into final image
        // 5. Optionally generate SVG from outlines
    }
}
```

### Day 8-9: SVG Output Support

#### Step 9: SVG Generation (crates/o4e-render/src/svg.rs)
```rust
use o4e_core::{Glyph, ShapingResult};

pub struct SvgRenderer {
    precision: usize,
    simplify: bool,
}

impl SvgRenderer {
    pub fn render(&self, shaped: &ShapingResult, options: &SvgOptions) -> String {
        let mut svg = String::new();

        // 1. Calculate bounding box
        let bbox = calculate_bbox(&shaped.glyphs);

        // 2. Write SVG header
        svg.push_str(&format!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}">"#,
            bbox.x, bbox.y, bbox.width, bbox.height
        ));

        // 3. Write each glyph as path
        for glyph in &shaped.glyphs {
            let path = extract_glyph_path(glyph);
            let simplified = if self.simplify {
                simplify_path(&path, self.precision)
            } else {
                path
            };

            svg.push_str(&format!(
                r#"<path d="{}" transform="translate({}, {})" />"#,
                simplified, glyph.x, glyph.y
            ));
        }

        // 4. Close SVG
        svg.push_str("</svg>");
        svg
    }
}

fn extract_glyph_path(glyph: &Glyph) -> String {
    // Extract Bézier curves from glyph outline
    // Convert to SVG path commands (M, L, Q, C)
}

fn simplify_path(path: &str, precision: usize) -> String {
    // Douglas-Peucker simplification
    // Remove redundant points
    // Round to precision
}
```

### Day 10: DirectWrite Backend (Windows)

#### Step 10: o4e-win Implementation
```rust
use o4e_core::*;
use windows::Win32::Graphics::{
    DirectWrite::*,
    Direct2D::*,
};

pub struct DirectWriteBackend {
    dwrite_factory: IDWriteFactory,
    d2d_factory: ID2D1Factory,
    font_cache: LruCache<String, IDWriteFontFace>,
}

impl Backend for DirectWriteBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        // Use IDWriteTextAnalyzer for:
        // 1. Script analysis
        // 2. Bidi analysis
        // 3. Line breaking
        // 4. Number substitution
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // 1. Create IDWriteTextFormat
        // 2. Create IDWriteTextLayout
        // 3. Apply features
        // 4. Get glyph runs
        // 5. Extract glyph data
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // 1. Create D2D render target
        // 2. Begin draw
        // 3. Draw glyph runs
        // 4. End draw
        // 5. Get bitmap or generate SVG
    }
}
```

## Week 3: Integration & Optimization

### Day 11-12: Batch Processing & Parallelization

#### Step 11: Batch Renderer (crates/o4e-render/src/batch.rs)
```rust
use rayon::prelude::*;
use o4e_core::*;

pub struct BatchRenderer {
    backend: Arc<dyn Backend>,
    thread_pool: ThreadPool,
}

impl BatchRenderer {
    pub fn render_batch(&self, items: Vec<BatchItem>) -> Vec<Result<RenderOutput>> {
        items
            .par_iter()
            .map(|item| {
                // 1. Segment text
                let runs = self.backend.segment(&item.text, &item.segment_options)?;

                // 2. Shape each run
                let shaped: Vec<ShapingResult> = runs
                    .iter()
                    .map(|run| self.backend.shape(run, &item.font))
                    .collect::<Result<Vec<_>>>()?;

                // 3. Render combined result
                let combined = combine_shaped_results(shaped);
                self.backend.render(&combined, &item.render_options)
            })
            .collect()
    }

    pub fn render_streaming(&self, receiver: Receiver<BatchItem>) -> Receiver<Result<RenderOutput>> {
        let (tx, rx) = channel();

        thread::spawn(move || {
            receiver
                .into_iter()
                .par_bridge()
                .map(|item| self.render_single(item))
                .for_each(|result| {
                    tx.send(result).ok();
                });
        });

        rx
    }
}
```

### Day 13-14: Performance Optimization

#### Step 12: Font Cache Optimization
```rust
use memmap2::Mmap;
use dashmap::DashMap;

pub struct FontCache {
    /// Memory-mapped font files
    mmap_cache: DashMap<PathBuf, Arc<Mmap>>,

    /// Parsed font faces
    face_cache: DashMap<FontKey, Arc<FontFace>>,

    /// Shaped text cache
    shape_cache: DashMap<ShapeKey, Arc<ShapingResult>>,

    /// Rendered glyph cache
    glyph_cache: DashMap<GlyphKey, Arc<RenderedGlyph>>,
}

impl FontCache {
    pub fn get_or_load_font(&self, path: &Path) -> Result<Arc<FontFace>> {
        // 1. Check face cache
        if let Some(face) = self.face_cache.get(path) {
            return Ok(face.clone());
        }

        // 2. Get or create memory map
        let mmap = self.mmap_cache.entry(path.to_owned())
            .or_try_insert_with(|| {
                let file = File::open(path)?;
                unsafe { Mmap::map(&file) }
            })?;

        // 3. Parse font face
        let face = Arc::new(parse_font(&mmap)?);

        // 4. Cache and return
        self.face_cache.insert(path.to_owned(), face.clone());
        Ok(face)
    }
}
```

## Week 3 Bonus: Unicode Infrastructure

- [x] Extract ICU-driven segmentation into a reusable `o4e-unicode` crate with its own test suite
- [x] Replace the bespoke HarfBuzz backend segmenter with the shared implementation
- [x] Adopt the shared segmenter inside the CoreText backend to unlock script itemization and bidi resolution
- [ ] Wire the DirectWrite backend to the shared segmenter for parity

## Week 4: Production Ready

### Day 15-16: Testing Suite

#### Step 13: Comprehensive Tests
```rust
// tests/integration.rs
#[test]
fn test_all_backends_consistency() {
    let text = "Hello مرحبا 世界";
    let font = Font::new("NotoSans", 48.0);

    let backends = vec![
        #[cfg(target_os = "macos")]
        Box::new(CoreTextBackend::new()),

        Box::new(HarfBuzzBackend::new()),
    ];

    let results: Vec<_> = backends
        .iter()
        .map(|backend| backend.render(text, &font, &default_options()))
        .collect();

    // Compare results for consistency
    for window in results.windows(2) {
        assert_similar(&window[0], &window[1], 0.95);
    }
}

#[test]
fn test_unicode_edge_cases() {
    let cases = vec![
        "👨‍👩‍👧‍👦", // Family emoji with ZWJ
        "é", // Combining marks
        "שָׁלוֹם", // Hebrew with vowels
        "لِلَّهِ", // Arabic with diacritics
        "น้ำ", // Thai with tone marks
    ];

    for text in cases {
        let result = render(text);
        assert!(result.is_ok());
        assert!(result.unwrap().glyphs.len() > 0);
    }
}

#[bench]
fn bench_simple_latin(b: &mut Bencher) {
    let backend = get_default_backend();
    let text = "The quick brown fox";
    let font = Font::new("Arial", 24.0);

    b.iter(|| {
        backend.render(text, &font, &default_options())
    });
}
```

### Day 17-18: Python Package Polish

#### Step 14: Python API Enhancement
```python
# o4e/__init__.py
from typing import Optional, Union, Dict, Any
from pathlib import Path
from PIL import Image
import numpy as np

from ._o4e import TextRenderer as _TextRenderer, Font as _Font

class Font:
    """CSS-style font specification."""

    def __init__(
        self,
        family: Union[str, Path],
        size: float = 16.0,
        weight: int = 400,
        style: str = "normal",
        variations: Optional[Dict[str, float]] = None,
        features: Optional[Dict[str, bool]] = None,
    ):
        self._font = _Font(family, size, weight, style, variations, features)

    @property
    def family(self) -> str:
        return self._font.family

    @property
    def size(self) -> float:
        return self._font.size

class TextRenderer:
    """High-performance multi-backend text renderer."""

    def __init__(
        self,
        backend: Optional[str] = None,
        cache_size: int = 512,
        parallel: bool = True,
    ):
        self._renderer = _TextRenderer(backend, cache_size, parallel)

    def render(
        self,
        text: str,
        font: Union[Font, str],
        output_format: str = "png",
        **options: Any
    ) -> Union[Image.Image, str, np.ndarray]:
        """Render text to specified format."""

        if isinstance(font, str):
            font = Font(font)

        result = self._renderer.render(text, font._font, output_format, options)

        if output_format in ["png", "jpeg", "webp"]:
            return Image.frombytes("RGBA", result.size, result.data)
        elif output_format == "svg":
            return result.svg
        elif output_format == "raw":
            return np.frombuffer(result.data, dtype=np.uint8).reshape(
                result.height, result.width, 4
            )
        else:
            return result

    def shape(self, text: str, font: Union[Font, str], **options: Any) -> ShapingResult:
        """Get shaping information without rendering."""

        if isinstance(font, str):
            font = Font(font)

        return self._renderer.shape(text, font._font, options)

    def render_batch(
        self,
        items: List[Dict[str, Any]],
        output_format: str = "png",
        max_workers: Optional[int] = None,
    ) -> List[Union[Image.Image, str, np.ndarray]]:
        """Efficiently render multiple texts in parallel."""

        return self._renderer.render_batch(items, output_format, max_workers)

# Convenience function
def render(text: str, font: Union[str, Font] = "Arial", size: float = 16.0, **kwargs):
    """Quick render function for simple use cases."""

    if isinstance(font, str):
        font = Font(font, size)

    renderer = TextRenderer()
    return renderer.render(text, font, **kwargs)
```

### Day 19-20: CI/CD Setup

#### Step 15: GitHub Actions Workflow
```yaml
# .github/workflows/ci.yml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    name: Test
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        rust: [stable, nightly]
    runs-on: ${{ matrix.os }}

    steps:
    - uses: actions/checkout@v4

    - name: Setup Rust
      uses: dtolnay/rust-toolchain@master
      with:
        toolchain: ${{ matrix.rust }}
        components: rustfmt, clippy

    - name: Cache cargo
      uses: Swatinem/rust-cache@v2

    - name: Check formatting
      run: cargo fmt -- --check

    - name: Clippy
      run: cargo clippy --all-targets --all-features -- -D warnings

    - name: Test
      run: cargo test --all-features

    - name: Benchmark
      run: cargo bench --no-run

  python:
    name: Python
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        python: ["3.8", "3.9", "3.10", "3.11", "3.12"]
    runs-on: ${{ matrix.os }}

    steps:
    - uses: actions/checkout@v4

    - name: Setup Python
      uses: actions/setup-python@v4
      with:
        python-version: ${{ matrix.python }}

    - name: Install maturin
      run: pip install maturin

    - name: Build wheel
      run: maturin build --release

    - name: Install wheel
      run: pip install target/wheels/*.whl

    - name: Test Python
      run: |
        pip install pytest pytest-benchmark pillow numpy
        pytest python/tests/ -v

  release:
    name: Release
    needs: [test, python]
    runs-on: ubuntu-latest
    if: startsWith(github.ref, 'refs/tags/v')

    steps:
    - uses: actions/checkout@v4

    - name: Setup Python
      uses: actions/setup-python@v4
      with:
        python-version: "3.12"

    - name: Build wheels
      uses: PyO3/maturin-action@v1
      with:
        command: build
        args: --release

    - name: Upload to PyPI
      uses: PyO3/maturin-action@v1
      with:
        command: upload
        args: --skip-existing
      env:
        MATURIN_PYPI_TOKEN: ${{ secrets.PYPI_TOKEN }}
```

## Directory Structure (Final)

```
o4e/
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── release.yml
├── backends/
│   ├── o4e-core/           # Shared traits and utilities
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── traits.rs
│   │       ├── types.rs
│   │       ├── cache.rs
│   │       └── error.rs
│   ├── o4e-mac/           # CoreText backend
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── segmenter.rs
│   │       ├── shaper.rs
│   │       └── renderer.rs
│   ├── o4e-win/           # DirectWrite backend
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   ├── o4e-icu-hb/        # ICU+HarfBuzz backend
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── segmenter.rs
│   │       ├── shaper.rs
│   │       └── renderer.rs
│   └── o4e-pure/          # Pure Rust backend
│       ├── Cargo.toml
│       └── src/
│           └── lib.rs
├── crates/
│   ├── o4e-api/           # Public API types
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   ├── o4e-unicode/       # Unicode processing
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── segmentation.rs
│   │       ├── bidi.rs
│   │       └── normalization.rs
│   ├── o4e-shaping/       # Text shaping abstraction
│   │   ├── Cargo.toml
│   │   └── src/
│   │       └── lib.rs
│   └── o4e-render/        # Rendering abstraction
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── batch.rs
│           ├── svg.rs
│           └── bitmap.rs
├── python/
│   ├── src/               # PyO3 bindings
│   │   ├── lib.rs
│   │   ├── font.rs
│   │   ├── renderer.rs
│   │   └── types.rs
│   ├── o4e/              # Python package
│   │   ├── __init__.py
│   │   ├── backends.py
│   │   ├── font.py
│   │   └── renderer.py
│   └── tests/            # Python tests
│       ├── test_basic.py
│       ├── test_backends.py
│       ├── test_unicode.py
│       └── test_performance.py
├── examples/
│   ├── basic/
│   │   ├── hello_world.rs
│   │   └── hello_world.py
│   ├── advanced/
│   │   ├── variable_fonts.rs
│   │   ├── complex_scripts.py
│   │   └── batch_processing.rs
│   └── benchmarks/
│       ├── throughput.rs
│       └── latency.py
├── tests/
│   ├── integration.rs
│   └── consistency.rs
├── benches/
│   ├── single_render.rs
│   └── batch_render.rs
├── Cargo.toml             # Workspace root
├── pyproject.toml         # Python package config
├── README.md
├── GOALS.md
├── PLAN.md
├── TODO.md
├── CHANGELOG.md
└── LICENSE

```

## Release Schedule

### v0.1.0 - MVP (End of Week 2)
- [x] haforu reference implementation
- [ ] CoreText backend (macOS)
- [ ] Basic Python bindings
- [ ] Simple API: render(text, font) -> image
- [ ] Documentation: README, basic examples

### v0.2.0 - Cross-Platform (End of Week 3)
- [ ] ICU+HarfBuzz backend
- [ ] DirectWrite backend (Windows)
- [ ] SVG output support
- [ ] Batch processing API
- [ ] Performance benchmarks

### v0.3.0 - Production Features (End of Week 4)
- [ ] Pure Rust backend
- [ ] Font fallback chains
- [ ] Advanced shaping options
- [ ] Comprehensive test suite
- [ ] CI/CD pipeline

### v1.0.0 - Stable Release (Week 8)
- [ ] All backends feature-complete
- [ ] Performance targets achieved
- [ ] Full documentation
- [ ] Security audit complete
- [ ] Published to crates.io and PyPI

## Performance Targets Verification

### Single Render Benchmarks
```bash
cargo bench --bench single_render
```

Expected results:
- Simple Latin (< 100 chars): < 0.5ms ✓
- Complex script (Arabic): < 2ms ✓
- CJK with fallback: < 3ms ✓
- SVG generation: < 1ms overhead ✓

### Batch Performance
```bash
cargo bench --bench batch_render
```

Expected results:
- 5000 renders: < 500ms (> 10,000/sec) ✓
- Memory usage: < 100MB ✓
- CPU scaling: Linear to 8 cores ✓

## Build Instructions

### Development Build
```bash
# Clone repository
git clone https://github.com/fontlaborg/o4e
cd o4e

# Build all backends
cargo build --all-features

# Build specific backend
cargo build -p o4e-mac

# Build Python package
maturin develop --release
```

### Release Build
```bash
# Optimized release build
cargo build --release --all-features

# Build Python wheels
maturin build --release

# Build for specific Python version
maturin build --release --interpreter python3.11
```

### Testing
```bash
# Run all tests
cargo test --all-features

# Run specific backend tests
cargo test -p o4e-mac

# Run Python tests
pytest python/tests/ -v

# Run benchmarks
cargo bench

# Run with logging
RUST_LOG=debug cargo test
```

## Platform-Specific Notes

### macOS
- Requires macOS 11.0+
- Xcode Command Line Tools required
- CoreText backend is default
- Best performance with Metal acceleration

### Windows
- Requires Windows 10 1903+
- Visual Studio 2019+ with C++ tools
- DirectWrite backend is default
- Enable ClearType for best quality

### Linux
- ICU and HarfBuzz required
- Install: `apt-get install libicu-dev libharfbuzz-dev`
- FreeType for rendering: `apt-get install libfreetype6-dev`
- HarfBuzz backend is default

### WebAssembly
- Use pure Rust backend
- No system dependencies
- Compile with: `wasm-pack build --target web`
- Limited to 4GB memory

## Debugging Guide

### Enable Detailed Logging
```bash
RUST_LOG=o4e=debug cargo run
RUST_LOG=o4e=trace cargo test failing_test
```

### Profile Performance
```bash
# CPU profiling
cargo build --release
perf record --call-graph=dwarf target/release/o4e
perf report

# Memory profiling
valgrind --tool=massif target/release/o4e
ms_print massif.out.*
```

### Visual Debugging
```rust
// Save intermediate results
if cfg!(debug_assertions) {
    shaped.save_debug_image("debug_shaped.png");
    rendered.save_debug_image("debug_rendered.png");
}
```

## Common Issues & Solutions

### Issue: Fonts not found
**Solution**: Set O4E_FONT_PATH environment variable
```bash
export O4E_FONT_PATH=/System/Library/Fonts:/usr/share/fonts
```

### Issue: Poor rendering quality
**Solution**: Enable subpixel antialiasing
```python
renderer = TextRenderer(antialias="subpixel", hinting="full")
```

### Issue: Slow first render
**Solution**: Pre-warm font cache
```python
renderer.prewarm(["Arial", "Helvetica", "NotoSans"])
```

### Issue: Memory usage growing
**Solution**: Limit cache size
```python
renderer = TextRenderer(cache_size=256)  # Limit to 256 fonts
```

## Contributing

### Code Style
```bash
# Format code
cargo fmt

# Check lints
cargo clippy --all-targets --all-features

# Format Python
black python/
isort python/
```

### Commit Messages
Use conventional commits:
- `feat:` New feature
- `fix:` Bug fix
- `perf:` Performance improvement
- `docs:` Documentation
- `test:` Tests
- `refactor:` Code refactoring

### Pull Request Process
1. Fork repository
2. Create feature branch
3. Make changes with tests
4. Ensure CI passes
5. Submit PR with description
6. Wait for review

## Next Steps After MVP

1. **Skia Backend**: GPU acceleration support
2. **Font Management**: System font discovery
3. **Layout Engine**: Multi-line text, paragraphs
4. **Effects**: Shadows, gradients, outlines
5. **Accessibility**: Screen reader metadata
6. **Web Demo**: WASM playground
7. **Documentation**: API docs, tutorials
8. **Community**: Discord, examples repository
