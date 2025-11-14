---
this_file: PLAN.md
---

# o4e Implementation Plan – Tri-Backend MVP

## Top Objective
Deliver a verifiable tri-backend MVP—CoreText (macOS), DirectWrite (Windows), and ICU+HarfBuzz (cross-platform)—with a unified Python API, SVG + bitmap outputs, and CI-backed releases. Every task below exists only to reach that objective quickly.

## Current State Snapshot
- Workspace, traits, cache, and Python bindings scaffolding exist; most modules still use approximations (e.g., dummy glyph IDs, placeholder rendering).
- CoreText backend compiles but fakes glyph extraction and lacks font fallback, variation axes, or real metrics.
- DirectWrite backend bootstraps factories yet still renders placeholder text, has no segmentation/shaping analysis, and misses ClearType & glyph run extraction.
- ICU+HarfBuzz backend wires HarfBuzz/tiny-skia but leaks font data, lacks font fallback, and does not expose true outline renderers or cache invalidation.
- SVG renderer, batch renderer, and documentation mention advanced features that are not implemented.

## Guiding Constraints
1. **Delete before add**: remove placeholders/stubs as soon as real code exists.
2. **Package-first**: rely on ICU, HarfBuzz, ttf-parser, tiny-skia, windows-rs, objc2 rather than custom code.
3. **Test-first**: each bullet below has an accompanying test requirement; no unchecked task ships without a test.
4. **Single-file focus**: keep modules under 200 lines where possible and add helper structs instead of nested logic.
5. **Verification workflow**: implement → add/extend test → `uvx hatch test` + `cargo test` → note outcome in `WORK.md`.

## Phase 1 – Backend Hardening (blocker for everything else)
### 1.1 CoreText backend (`backends/o4e-mac`)
1. [x] Replace fake glyph creation with real `CTRun` extraction (`CTLine::glyph_runs`, `CTRunGetGlyphs`, `CTRunGetPositions`). Store glyph IDs/positions/advances in `ShapingResult.glyphs`.
2. [x] Add variation axis + feature propagation: build `CTFontDescriptor` with weight/width/style/variation data from `Font{weight,style,variations}` before creating `CTFont` instances. Cache by `(family, size, variations)`.
3. [x] Implement per-run font fallback: when `segmenter` marks `TextRun.font=None`, resolve through CoreText font descriptors and annotate runs with actual fonts so render() no longer panics.
4. [x] Rendering correctness: draw shaped glyphs via `CTFontDrawGlyphs` instead of redrawing text. Honour `RenderOptions.antialias` (map to CoreGraphics settings) and `background`.
5. [x] Tests: unit test glyph extraction for Latin/Arabic strings via `#[cfg(target_os="macos")]`; integration test ensures advance width equals CTLine typographic bounds; add PNG snapshot smoke test saved under `testdata/expected/coretext/`.

### 1.2 DirectWrite backend (`backends/o4e-win`)
1. [x] Segmentation + analysis: wire `IDWriteTextAnalyzer1::AnalyzeScript`, `AnalyzeBidi`, and `AnalyzeLineBreakpoints` to emit `TextRun`s with correct script/direction/line info.
2. [x] Glyph shaping: obtain `IDWriteGlyphRun` data through `GetGlyphRun` callbacks, populate real glyph IDs/advances, capture clusters, and store baseline metrics in `ShapingResult`.
3. [x] Rendering fidelity: replace placeholder "Hello World" drawing with `DrawGlyphRun` using the shaped data; expose ClearType and grayscale via `RenderOptions.antialias`.
4. [x] Feature toggles: extend `Font` -> `DWRITE_FONT_FEATURE` mapping (liga/kern/smcp) and variable font axes using `IDWriteFontFace5` when available.
5. Tests: add Rust integration test guarded by `#[cfg(target_os="windows")]` using known system font (e.g., `Segoe UI`) plus mock fonts from `testdata/fonts`; verify bidi segmentation and ClearType toggle produce different alpha coverage by hashing pixel buffer.

### 1.3 ICU+HarfBuzz backend (`backends/o4e-icu-hb`)
1. [x] Fix font lifetime handling: stop leaking font bytes by keeping `Arc<Vec<u8>>` in cache and using `Owned<FONT>` with `'static` by storing the `Arc` inside the struct.
2. [x] Implement glyph cache reuse: hook `FontCache::get_glyph`/`cache_glyph` so repeated renders reuse bitmaps; respect quantized size.
3. [x] Outline extraction: connect `ttf-parser` outlines into shared path builder that both raster (tiny-skia) and SVG renderer can reuse; expose `extract_glyph_path` helper returning `kurbo::BezPath` instructions.
4. [x] Font fallback + script-specific shaping: when `TextSegmenter` splits different scripts, locate fallback font per script (Noto fallback list) and attach to run before calling HarfBuzz.
5. [x] Tests: extend existing segmentation tests to ensure fallback fonts resolve; add shaping regression tests for Arabic/Devanagari strings comparing glyph ID sequences against HarfBuzz reference output stored in JSON fixtures.

### 1.4 Shared backend services
1. [x] Move system font discovery + fallback lists into a new crate `crates/o4e-fontdb` (or reuse `font-kit` if adequate) to avoid duplicating search logic across backends.
2. [x] Expand `Font` struct to capture `source: FontSource` (family vs path vs bytes) and update all backends to handle each variant.
3. [x] Implement `Backend::clear_cache` tests ensuring caches actually shrink (hit counters reset) to prevent leaks (added `FontCache::is_empty` diagnostics plus HarfBuzz/CoreText/DirectWrite fixtures).
4. Provide `RenderOptionsDiagnostics` struct (crate-local) logging actual backend + feature set for easier debugging; integrate with `log` macros only when `RUST_LOG` enabled.

## Phase 2 – Rendering & Outputs
### 2.1 SVG + outline pipeline (`crates/o4e-render`)
1. [x] Implement `extract_glyph_path` by sharing outlines from ICU+HB / DirectWrite / CoreText font data. Use `ttf-parser` to convert to `kurbo::BezPath` or raw `svgtypes::PathSegment`.
2. Add path simplification using `lyon_path::geom::euclid::default` (or `kurbo::PathEl::simplify`) with tolerance derived from `SvgOptions.precision`.
3. Support color fonts: detect COLRv1/CPAL tables and emit `<g>` per palette layer; fallback to solid color otherwise.
4. Tests: snapshot-test SVG output using `insta` for Latin, CJK, emoji; verify `<path>` count matches glyph count and bounding boxes align.

### 2.2 Raster pipeline & batch renderer
1. Introduce `RenderSurface` abstraction (enum) so CoreText/DirectWrite/ICU+HB can share conversion logic (BGRA→RGBA, premultiplied alpha checks).
2. Expand batch renderer progress to include latency percentiles; feed from `rayon::ParallelIterator` instrumentation.
3. Add benchmarking harness under `benches/` (criterion) comparing single vs batch render throughput for each backend; store results in `perf/benchmarks.md`.
4. Tests: property test combining shaped results ensures glyph offsets never regress; add stress test to ensure `BatchRenderer::render_batch_with_threads` handles >10k items without panic.

## Phase 3 – Python API & Packaging
### 3.1 API surface completion (`python/src/lib.rs`, `python/o4e/__init__.py`)
1. Auto backend selection order: mac→CoreText, Windows→DirectWrite, else HarfBuzz; allow `backend="harfbuzz"` override even on mac/windows.
2. [x] Support user-provided font bytes/paths via `Font.from_path`/`Font.from_bytes` factory; plumb through to Rust `FontSource`.
3. Expose `TextSegmenter` class + streaming batch API; ensure docstrings map to Rust types and mention return types.
4. Add `__repr__`/`__richrepr__` for debugging, plus `to_pillow` helper for `RenderOutput::Bitmap`.
5. Tests: `python/tests/test_api.py` should cover render, shape, batch, fallback fonts, error handling; use fixtures referencing small SIL fonts stored under `python/tests/fonts`.

### 3.2 Distribution & extras
1. Update `pyproject.toml` optional extras to pull correct wheels (`mac`, `windows`, `icu`, `skia`), and document environment markers.
2. Add `build.rs` gating so maturin builds only relevant backends per platform; ensure features map to extras.
3. Provide `examples/` scripts (PNG render, SVG export, shaping dump) that double as functional tests executed via `./test.sh`.
4. Tests: `uvx hatch test` must run on CI (mac+windows+linux) and ensure extras install; add `pip install .[mac]` smoke test job in GitHub Actions.

### 3.3 Documentation & developer UX
1. Keep `README.md` under 200 lines summarizing API + installation; link to `GOALS.md`/`PLAN.md`.
2. Update `CHANGELOG.md`, `WORK.md`, `DEPENDENCIES.md` after every meaningful change; ensure `this_file` headers remain accurate.
3. Author `docs/backends.md` describing backend capabilities, limitations, and how to select them from Python/Rust.

## Phase 4 – Quality, CI, and Releases
1. **Testing matrix**: macOS (Intel + Apple Silicon), Windows (10/11), Linux (Ubuntu LTS). For each, run `cargo fmt`, `cargo clippy --all-targets --all-features`, `cargo test`, and `uvx hatch test`.
2. **Fuzzing**: add `cargo fuzz` target for glyph outline parsing and HarfBuzz feature inputs; run nightly in CI.
3. **CI/CD**: update `.github/workflows/ci.yml` to build all crates, run tests, upload coverage; add `release.yml` job triggered by tags to publish crates (via `cargo publish --locked`) and Python wheels (`maturin publish`).
4. **Release checklist**: smoke test `examples/`, update `CHANGELOG.md`, tag (`git tag v0.1.0`), run workflows, verify PyPI + crates.io artifacts, archive sample renders under `artifacts/`.

## Dependencies & Tooling Map
- **CoreText**: `objc2`, `core-foundation`, `core-text`, `core-graphics`.
- **DirectWrite**: `windows` crate with `Win32_Graphics_{DirectWrite,Direct2D}`, `Win32_System_Com`.
- **ICU/HarfBuzz**: `harfbuzz_rs`, `icu_segmenter`, `icu_locid`, `unicode-bidi`, `ttf-parser`, `tiny-skia`.
- **SVG/Outlines**: `kurbo`, `svgtypes`, `lyon_geom`/`lyon_path`.
- **Batch/Perf**: `rayon`, `criterion`, `insta`.
- **Python bindings**: `pyo3`, `maturin`, `rich`, `fire` (for CLI scripts), `loguru` for logging.

## Verification Strategy
1. **Unit tests**: each backend module gets targeted tests (`cargo test -p o4e-mac`, etc.) covering glyph extraction, segmentation, render output shape.
2. **Integration tests**: cross-backend golden tests compare shaping JSON + PNG hashes; run via `cargo test -p o4e-render --all-features`.
3. **Python tests**: `uvx hatch test` (pytest) referencing fixtures; ensure `PYTHONUTF8=1` set for consistent encoding.
4. **Benchmarks**: `cargo bench -p o4e-render` and `cargo criterion --bench batch_render` after backend changes; record results in `WORK.md`.
5. **Manual QA**: `examples/convert_to_png.py` + `examples/basic_render.py` executed on each platform, visual diffs stored alongside outputs for release candidates.

Keeping this plan current is mandatory—update sections whenever scope shifts, and mirror actionable items in `TODO.md` as single-line checkboxes.
