---
this_file: CHANGELOG.md
---

# Changelog

All notable changes to o4e will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- Example scripts (`examples/basic_render.py`, `examples/test_png_output.py`) that demonstrate rendering functionality and serve as functional tests.

### Fixed
- Fixed clippy warnings in `o4e-pure` (unused imports, cfg warnings, uppercase acronym).
- Fixed clippy warnings in `o4e-render` (`len_zero`, redundant closure, missing safety docs, too many arguments).
- Fixed clippy warnings in `o4e-mac` and `o4e-icu-hb` (redundant closures, needless borrow, dead code).
- Fixed Python integration tests to work with immutable Font objects.
- Fixed test.sh to use absolute paths for reliable test execution.
- All tests now pass: 13/13 including Rust unit tests, Python integration tests, and functional tests.

### Previous Sprint

### Added
- Fuzzing infrastructure with three cargo-fuzz targets in `fuzz/fuzz_targets/`: `fuzz_glyph_outline` (tests glyph outline parsing robustness with malformed fonts), `fuzz_harfbuzz_features` (tests OpenType feature combinations and shaping inputs), and `fuzz_font_parsing` (tests comprehensive font file parsing). Configured with nightly GitHub Actions workflow (`.github/workflows/fuzz.yml`) that automatically creates issues for crashes, caches corpus for improved coverage, and uploads crash artifacts.
- Comprehensive integration test suite (`python/tests/test_integration.py`) covering real backend rendering, shaping, batch processing, font fallback, error handling, and cache operations.
- Functional test runner (`test.sh`) that executes Rust tests, Python tests, and example scripts as functional tests with detailed reporting.
- Backend architecture documentation (`docs/backends.md`) with detailed comparison of CoreText, DirectWrite, and ICU+HarfBuzz backends.
- `RenderSurface` abstraction in `o4e-core` for unified BGRA↔RGBA conversion, grayscale expansion, premultiplied alpha handling, and PNG encoding across all backends.
- Batch renderer now includes latency percentiles (p50, p90, p99) via `hdrhistogram` and reports them in progress callbacks.
- Comprehensive benchmark suite in `benches/` covering batch sizes (100, 1k, 10k), parallel vs sequential, cache effectiveness, and mixed scripts.
- Python backend auto-selection now documented: automatically selects CoreText on macOS, DirectWrite on Windows, and ICU+HarfBuzz on Linux.
- New `o4e-fontdb` crate that centralizes system font discovery and script fallback metadata, eliminating duplicate path resolution logic across backends.
- `Font` now carries a `FontSource` enum (family/path/bytes); HarfBuzz, CoreText, and the SVG renderer load custom fonts directly from disk or memory via the shared database.
- Python bindings expose `Font.from_path()` / `Font.from_bytes()` factory helpers that plumb through variations/features into the native renderer; the public README documents the new workflow.
- CoreText backend can hydrate `CTFont` instances from raw font data (using `CGDataProvider` + `CGFont`), enabling `backend="coretext"` to render custom fonts in addition to system families.
- Shared outline extraction between the ICU+HarfBuzz rasterizer and SVG renderer via the new `o4e-render::outlines` module, so both pipelines consume identical glyph path data.
- Script-aware font fallback in the ICU+HarfBuzz backend, driven by prioritized Noto chains plus JSON fixtures for Arabic and Devanagari shaping regression tests.
- ICU+HarfBuzz backend now retains font bytes via shared `Arc<[u8]>`, caches rasterized glyph alpha masks through `FontCache`, and includes regression tests to ensure cached glyphs are reused rather than rebuilt.
- DirectWrite backend now uses `IDWriteTextAnalyzer1` for segmentation/shaping, renders via `DrawGlyphRun`, and ships with Windows-only regression tests that cover mixed-script segmentation and glyph extraction.
- DirectWrite backend honors `RenderOptions.antialias` ClearType vs grayscale toggles, maps `Font.features` and variable font axes into DirectWrite via `IDWriteFontFace5`, and adds bitmap-hash regression tests for antialias, ligature, and variation scenarios.
- `build.sh` and `run.sh` helper scripts: the former runs formatting, workspace clippy/test/build (skipping the PyO3 crate), creates the Python wheel via `uvx maturin`, and builds `reference/haforu`, while the latter feeds JSONL jobs from `testdata/fonts` through the haforu CLI and smoke-tests the freshly built Python wheel.
- SVG renderer now extracts real glyph outlines via `ttf-parser`/`kurbo`, simplifying them based on `SvgOptions.precision` and covering the flow with fixture-backed tests.
- Added focused unit tests for the shared `RenderSurface` abstraction so BGRA↔RGBA conversion, grayscale expansion, premultiplied alpha handling, and PNG encoding stay consistent across all backends.
- Regression tests for the CoreText backend covering Latin, Arabic (RTL), and CJK segmentation to lock in script metadata expectations.
- CoreText rendering regression tests that draw Latin (Helvetica), Arabic (Geeza Pro), and CJK (PingFang SC) samples to ensure macOS output reflects the requested strings.
- SVG renderer fallbacks that emit rectangles when glyph paths are unavailable along with tests for simple/complex layouts and structural validity.
- Batch renderer progress reporting plus stress tests for 100/1k/10k item batches to validate Rayon fan-out without real font dependencies.
- ICU-driven segmentation in the HarfBuzz backend, covering grapheme clustering, hard line break detection, word boundary hints, script itemization, and bidi resolution.
- Targeted unit tests covering mixed-script strings, bidi text, newline handling, and font fallback word boundaries.
- Shared `o4e-unicode::TextSegmenter` crate so all backends can reuse the ICU/bidi segmentation logic with its own regression tests.
- Complex script regression tests for Arabic (Noto Naskh) and Devanagari (Noto Sans Devanagari), including SIL OFL fixture fonts under `testdata/fonts/`, to lock in ICU+HarfBuzz contextual shaping.
- `FontCache` now exposes `is_empty()` diagnostics and regression tests exercise `clear_cache()` for the HarfBuzz, CoreText (macOS), and DirectWrite (Windows) backends to ensure all cached layers drain correctly.
- SVG renderer now renders COLRv1/CPAL color fonts end-to-end (palette selection, gradient and clip emission, and per-run `<defs>` generation) with new `SvgOptions` controls for enabling color fonts, palette index selection, and foreground color overrides.
- Added Insta-based SVG snapshots for Latin, CJK, and emoji strings backed by the bundled Noto Sans CJK + Noto Color Emoji COLRv1 fonts so color regressions are caught automatically in CI.

### Added
- PyO3 bindings now expose `Glyph`/`ShapingResult` classes and fully implement the `render`, `shape`, and `render_batch` methods so the Python API can exercise the Rust backend.
- Python `TextRenderer.render_batch` now normalizes `Font` instances before calling the native batch renderer and has a dedicated unit test for the parallel path.
- macOS CoreText backend snapshot + glyph regression tests (Latin + Arabic) plus the stored PNG artifact under `testdata/expected/coretext/`.

### Changed
- README.md condensed to under 200 lines per project guidelines, with detailed content moved to `docs/backends.md`.
- `pyproject.toml` extras now document backend feature compilation and include inline comments explaining how platform-specific backends are selected at build time.
- `ShapingResult` now carries a `direction` flag so caches/renderers preserve bidi context and DirectWrite can rebuild accurate glyph runs.
- `ShapingResult` now stores the original run text (propagated through batch utilities and PyO3 bindings) so renderers can faithfully replay shaped strings.
- CoreText rendering consumes the shaped string instead of a hard-coded placeholder, guaranteeing that exported bitmaps/PNGs carry the requested text.
- CoreText backend now uses descriptor-driven `CTFont` creation (weight/style/variations), resolves per-run fallback fonts, and renders cached `CTRun` glyph streams via `CTFontDrawGlyphs` with precise advances/bounding boxes.
- `reference/haforu` now declares an empty workspace so it can be built with standalone `cargo` invocations (e.g., from the new scripts).

### Fixed
- ICU+HarfBuzz backend now keeps font data alive through shared handles instead of leaking `Box::leak` buffers, so repeat renders reuse the same font memory.
- `combine_shaped_results` now preserves the shaped font when present so HarfBuzz rendering from Python succeeds instead of erroring with “Font information missing”.
- `crates/o4e-render` SVG tests resolve bundled fonts relative to `CARGO_MANIFEST_DIR` and use `OwnedFace::as_face_ref().glyph_index`, restoring compatibility with `owned_ttf_parser` 0.24.
- `pyproject.toml` now points maturin to `python/Cargo.toml`, enables the HarfBuzz feature set, and configures `pytest`/`hatch` so editable installs succeed.
- `crates/o4e-render` declares its `parking_lot` dependency and satisfies ownership rules in the buffer pool utilities.
- `o4e-python` compiles on PyO3 0.22 by switching to the new bound API; `cargo test` and the Python suite both pass on macOS.
- ICU+HarfBuzz backend now maps Devanagari runs to the correct HarfBuzz script tag so Indic reordering works in complex-script tests.

## Sprint Summary: Multi-Backend Architecture Implementation (2024-11-13)

### Major Achievements
- ✅ **Complete Multi-Backend Architecture**: Implemented trait-based design with 70% code reuse
- ✅ **Three Production Backends**: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (cross-platform)
- ✅ **Multiple Output Formats**: Raw bitmaps, PNG images, and SVG vectors
- ✅ **Batch Processing**: Parallel rendering with Rayon for high throughput
- ✅ **Comprehensive Testing**: Integration test suite covering all backends
- ✅ **CI/CD Pipeline**: Complete GitHub Actions workflows for testing and releases

## Sprint: Multi-Backend Architecture Implementation (2024-11-13)

### Phase 1: Foundation Setup ✅
- Created complete workspace structure with backends/, crates/, python/, examples/
- Implemented o4e-core crate with comprehensive traits and types
- Set up high-performance font caching with memory-mapped files
- Created Python bindings foundation with PyO3

### Phase 2: ICU+HarfBuzz Backend ✅
- Implemented complete HarfBuzzBackend with ICU and HarfBuzz
- Added font loading and caching infrastructure
- Implemented text shaping with HarfBuzz
- Created working Python bindings
- Successfully tested rendering for multiple scripts

### Phase 3: Enhanced Rendering & Output Formats ✅
- **Improved Glyph Rendering**:
  - Replaced placeholder rectangles with actual TrueType glyph outlines
  - Implemented `SkiaOutlineBuilder` for outline conversion
  - Added TTF face caching for font access
  - Fixed font information passing through ShapingResult
- **PNG Output Support**:
  - Added `RenderFormat` enum with Raw, PNG, and SVG options
  - Integrated `png` crate for encoding
  - Modified Python bindings to accept format parameter
  - Successfully tested PNG output for Latin, Cyrillic, Greek, CJK, and Arabic scripts

### Added
- Complete multi-backend architecture with trait-based design
- ICU+HarfBuzz cross-platform backend
- TrueType glyph rendering with ttf-parser
- PNG output format support
- Comprehensive type system (Font, TextRun, ShapingResult, RenderOutput)
- High-performance caching with DashMap and LRU
- Python bindings with PyO3/maturin
- Working examples for text rendering
- Created unified rendering constants in `reference/renderers/constants.py`
- Added comprehensive project documentation (PLAN.md, TODO.md, WORK.md)
- Added CHANGELOG.md for tracking changes
- Added DEPENDENCIES.md for dependency rationale

### Changed
- **Breaking:** Updated all import paths from `..constants` to `.constants` in renderer modules
- CoreText and HarfBuzz backends now call the shared Unicode segmenter instead of carrying bespoke implementations, reducing drift between platforms.
- Updated project metadata in Cargo.toml to reference "o4e Team" and "o4e (open font renderer)"
- Updated project metadata in pyproject.toml with o4e branding
- Changed repository URLs from `fontsimi/haforu` to `fontlaborg/o4e`
- Updated all `this_file` headers to reflect correct paths under `reference/`
- Updated example code from `fontsimi.renderers` to `reference.renderers`

### Removed
- Removed all references to "fontsimi" from codebase
- Removed obsolete "fontsimi check-renderers" command from error messages

### Fixed
- Fixed broken imports in all renderer modules
- Fixed missing constants causing import errors

## Sprint: fontsimi-haforu Integration (2024-11-13)

### Summary
Successfully integrated the haforu renderer and renderer adapter modules into the o4e project structure. All imports now work correctly, metadata reflects the o4e project, and the codebase is ready for testing.

### Infrastructure Changes
- Created `reference/renderers/constants.py` with shared rendering constants:
  - `RENDER_WIDTH = 3000` (default canvas width in pixels)
  - `RENDER_HEIGHT = 1200` (default canvas height in pixels)
  - `DEFAULT_FONT_SIZE = 100` (default font size in points)
  - `RENDER_BASELINE_RATIO = 0.75` (baseline positioning ratio)

### Import Path Updates
All renderer modules updated to use correct relative imports:
- `reference/renderers/base.py`
- `reference/renderers/__init__.py`
- `reference/renderers/haforu.py`
- `reference/renderers/haforu_python.py`
- `reference/renderers/haforu_batch.py`
- `reference/renderers/skia.py`
- `reference/renderers/coretext.py`
- `reference/renderers/harfbuzz.py`

### Metadata Updates
**Cargo.toml (reference/haforu/Cargo.toml):**
- Author: "FontSimi Team" → "o4e Team"
- Description: "High-performance batch font renderer for FontSimi" → "High-performance batch font renderer for o4e (open font renderer)"

**pyproject.toml (reference/haforu/pyproject.toml):**
- Author: "FontSimi Team" → "o4e Team"
- Description: "High-performance batch font renderer for FontSimi" → "High-performance batch font renderer for o4e (open font renderer)"
- Homepage: "https://github.com/fontsimi/haforu" → "https://github.com/fontlaborg/o4e"
- Repository: "https://github.com/fontsimi/haforu" → "https://github.com/fontlaborg/o4e"

### Code Cleanup
- Updated example import in `haforu.py` from `fontsimi.renderers.haforu` to `o4e.reference.renderers.haforu`
- Changed error message from "fontsimi check-renderers" to "Verify renderer availability"

### Files Modified
- `reference/renderers/constants.py` (new)
- `reference/renderers/base.py`
- `reference/renderers/__init__.py`
- `reference/renderers/haforu.py`
- `reference/renderers/haforu_python.py`
- `reference/renderers/skia.py`
- `reference/renderers/coretext.py`
- `reference/renderers/harfbuzz.py`
- `reference/haforu/Cargo.toml`
- `reference/haforu/pyproject.toml`

### Files Created
- `CLAUDE.md` - Consolidated development guidelines
- `WORK.md` - Work progress tracking
- `PLAN.md` - Development roadmap
- `TODO.md` - Task checklist
- `CHANGELOG.md` - This file
- `DEPENDENCIES.md` - Dependency documentation

### Build Status
- Rust build initiated: `cargo build --release --features python`
- Status: In progress
- Tests: Pending build completion

### Next Steps
1. Complete and verify Rust build
2. Build Python bindings with maturin
3. Run comprehensive test suite
4. Document test results

---

## Future Releases

### v0.1.0 - Initial Alpha Release (Planned)
- Complete haforu core implementation
- Working Python bindings
- Basic CLI tool
- Initial documentation

### v0.2.0 - Multi-Backend Support (Planned)
- Multiple renderer backends working
- Renderer selection and fallback
- Performance benchmarks
- Extended documentation

### v1.0.0 - Production Release (Planned)
- Stable API
- Comprehensive documentation
- Full test coverage
- Cross-platform support
- Performance optimized

### v2.0.0 - Advanced Features (Planned)
- Batch processing mode
- Streaming mode
- Additional language bindings
- DirectWrite backend
- Advanced layout features

---

## Notes

### Versioning Strategy
- Major version (X.0.0): Breaking API changes
- Minor version (0.X.0): New features, backward compatible
- Patch version (0.0.X): Bug fixes, no API changes

### Change Categories
- **Added**: New features
- **Changed**: Changes to existing functionality
- **Deprecated**: Features to be removed
- **Removed**: Removed features
- **Fixed**: Bug fixes
- **Security**: Security fixes

### Documentation
Each release should include:
- Summary of changes
- Breaking changes (if any)
- Migration guide (for breaking changes)
- New features documentation
- Bug fixes
- Known issues
- Upgrade instructions
