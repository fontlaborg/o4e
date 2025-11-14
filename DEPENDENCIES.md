---
this_file: DEPENDENCIES.md
---

# Dependencies

This document lists all project dependencies and explains why each was chosen.

## Rust Dependencies (haforu)

### Core Font Handling

#### `read-fonts = "0.22"` & `skrifa = "0.22"`
**Purpose:** Font parsing and glyph outline extraction
**Why chosen:**
- Part of Google's fontations ecosystem (modern, safe Rust font library)
- Pure Rust implementation (no C dependencies)
- Excellent performance and memory safety
- Well-maintained by Google Fonts team
- Supports OpenType, TrueType, variable fonts
- Better than older alternatives (ttf-parser, font-kit)

#### `memmap2 = "0.9"`
**Purpose:** Memory-mapped file I/O for zero-copy font loading
**Why chosen:**
- Essential for performance with large font files
- Zero-copy access to font data
- Reduces memory footprint
- Standard choice for high-performance file access in Rust
- Well-maintained and widely used

#### `lru = "0.12"`
**Purpose:** LRU cache for font instances
**Why chosen:**
- Simple, efficient LRU implementation
- Prevents redundant font loading
- Critical for performance with variable fonts (many coordinate combinations)
- Minimal overhead
- Standard crate for this use case

#### `fontdb = "0.23"`
**Purpose:** Shared system-font discovery and fallback data
**Why chosen:**
- Provides a cross-platform database over system font registries
- Exposes CSS-style queries (family, weight, stretch) with fallback lists
- Lets us resolve family names, file paths, or in-memory blobs without duplicating logic per backend
- Pure Rust, battle-tested inside Servo/COSMIC text stack
- Simpler than wiring `font-kit` everywhere and light enough to embed inside our own `o4e-fontdb` helper crate

### Text Shaping

#### `harfbuzz_rs = "2.0"`
**Purpose:** Text shaping (glyph selection and positioning)
**Why chosen:**
- Industry-standard shaping library
- Handles complex scripts (Arabic, Indic, etc.)
- OpenType feature support
- Bidirectional text support
- Used by Chrome, Firefox, Android, etc.
- Safe Rust bindings over battle-tested C library
- No viable pure-Rust alternative

### Rasterization

#### `zeno = "0.3"`
**Purpose:** Glyph rasterization (converting outlines to pixels)
**Why chosen:**
- Pure Rust implementation
- Fast software rasterizer
- Good quality antialiasing
- No GPU required
- Simpler than alternatives (fontdue, ab_glyph)
- Designed for font rendering use case

#### `kurbo = "0.11"`
**Purpose:** Shared Bézier path representation for glyph outlines (SVG + raster backends)
**Why chosen:**
- Provides a consistent `BezPath` type for converting `ttf-parser` outlines into reusable geometry
- Used heavily in the Druid/piet ecosystem; well maintained and numerically stable
- Zero native dependencies; perfect for sharing outline logic across crates
- Lets us simplify paths, emit SVG commands, and convert into tiny-skia paths without bespoke builders
- Cleaner than keeping duplicate outline recorders per backend

### Image Output

#### `image = "0.25"` (features: png, jpeg)
**Purpose:** Image encoding and manipulation
**Why chosen:**
- De facto standard for image handling in Rust
- PNG encoding required for output
- JPEG optional for compressed output
- Pure Rust PNG encoder
- Well-maintained and widely used

#### `base64 = "0.22"`
**Purpose:** Base64 encoding for JSONL image output
**Why chosen:**
- Standard for embedding binary data in JSON
- Fast, minimal implementation
- Widely used and maintained

### Serialization

#### `serde = "1.0"` (features: derive) & `serde_json = "1.0"`
**Purpose:** JSON serialization/deserialization
**Why chosen:**
- De facto standard for Rust serialization
- Required for JSONL input/output format
- Derive macros reduce boilerplate
- Excellent performance
- Ecosystem standard

### Error Handling

#### `thiserror = "1.0"`
**Purpose:** Derive macro for error types
**Why chosen:**
- Standard for library error types
- Reduces boilerplate
- Integrates with std::error::Error
- Better than manual implementations

#### `anyhow = "1.0"`
**Purpose:** Application-level error handling with context
**Why chosen:**
- Perfect for CLI and application code
- Easy to add context to errors
- Backtrace support
- Standard choice for Rust apps

### CLI and Arguments

#### `clap = "4.5"` (features: derive, cargo)
**Purpose:** Command-line argument parsing
**Why chosen:**
- Most popular Rust CLI framework
- Derive macros for clean API
- Auto-generated help text
- Validation and type safety
- cargo feature provides version info

### Logging

#### `log = "0.4"` & `env_logger = "0.11"`
**Purpose:** Logging infrastructure
**Why chosen:**
- `log` is the standard logging facade
- `env_logger` provides simple configuration via env vars
- Minimal overhead when logging disabled
- Ecosystem standard

### Parallel Processing

#### `rayon = "1.10"`
**Purpose:** Data parallelism for batch processing
**Why chosen:**
- De facto standard for parallelism in Rust
- Work-stealing scheduler
- Easy to use (parallel iterators)
- Excellent performance
- Essential for batch mode scalability

### Synchronization

#### `parking_lot = "0.12"`
**Purpose:** High-performance locking primitives for shared structures
**Why chosen:**
- Provides `RwLock`/`Mutex` implementations with far lower overhead than the std versions
- Used by the buffer pool and performance metrics inside `crates/o4e-render`
- Mature, well-maintained crate adopted across the Rust ecosystem
- Drop-in API kept changes localized to the rendering utility crate

### Path Utilities

#### `camino = "1.1"`
**Purpose:** UTF-8 path handling
**Why chosen:**
- Enforces UTF-8 paths (required for cross-platform font paths)
- Better API than std::path::Path for our use case
- Serde integration for JSON serialization
- Prevents path encoding issues

### Outline Extraction & Font Resolution

#### `ttf-parser = "0.24"` & `owned_ttf_parser = "0.24"`
**Purpose:** Access TrueType/OpenType glyph outlines for SVG emission
**Why chosen:**
- Battle-tested font parser used across the Rust ecosystem
- `owned_ttf_parser` keeps font data alive without leaking allocations
- Provides direct access to glyph curves, units-per-em, and metrics needed for accurate scaling
- Lightweight dependency that aligns with future outline sharing between raster and SVG paths

#### `shellexpand = "3"`
**Purpose:** Expand `~` and environment variables in user-provided font paths
**Why chosen:**
- Minimal, well-maintained crate focused on safe shell-style expansion
- Avoids reimplementing brittle tilde handling logic
- Keeps font lookup consistent with the ICU+HB backend and cross-platform font discovery code

### Python Bindings

#### `pyo3 = "0.22"` (optional, features: extension-module)
**Purpose:** Python bindings via FFI
**Why chosen:**
- De facto standard for Rust-Python interop
- Safe, ergonomic API
- Excellent performance
- Active development
- Used by major projects (polars, ruff, etc.)

#### `numpy = "0.22"` (optional)
**Purpose:** NumPy array integration
**Why chosen:**
- Essential for Python image data exchange
- Zero-copy where possible
- Standard for numerical Python/Rust interop
- Complements pyo3

### Build System

#### `maturin >= 1.0, < 2.0` (build dependency)
**Purpose:** Building Python wheels from Rust
**Why chosen:**
- Standard tool for PyO3 projects
- Handles cross-compilation
- Publishes to PyPI
- Integrates with pip and poetry
- Active development

### Development Dependencies

#### `tempfile = "3.10"`
**Purpose:** Temporary file handling in tests
**Why chosen:**
- Essential for testing file I/O
- Automatic cleanup
- Cross-platform
- Standard test utility

#### `approx = "0.5"`
**Purpose:** Floating-point comparisons in tests
**Why chosen:**
- Required for comparing render results
- Handles numerical precision issues
- Standard for floating-point tests

#### `insta = "1.39"`
**Purpose:** Snapshot testing
**Why chosen:**
- Excellent for testing complex outputs (JSON, images)
- Automatic snapshot management
- Review workflow
- Reduces test boilerplate

## Python Dependencies

### Core

#### `numpy >= 1.20`
**Purpose:** Array handling for image data
**Why chosen:**
- Standard for numerical computing in Python
- Required for image manipulation
- Efficient array operations
- Universal API

### Renderer Backends

#### `uharfbuzz`
**Purpose:** HarfBuzz Python bindings
**Why chosen:**
- Official HarfBuzz Python bindings
- Required for HarfBuzzRenderer
- Well-maintained
- Cython-based (fast)

#### `freetype-py`
**Purpose:** FreeType Python bindings
**Why chosen:**
- Required for HarfBuzzRenderer (rasterization)
- Low-level access to FreeType
- Necessary for glyph rendering

#### `skia-python`
**Purpose:** Skia graphics library bindings
**Why chosen:**
- High-quality rendering
- Cross-platform
- Used by Chrome, Flutter
- Hardware acceleration support

#### `pyobjc` (macOS only)
**Purpose:** Access to macOS CoreText APIs
**Why chosen:**
- Required for CoreTextRenderer on macOS
- Native platform rendering
- Best quality on macOS
- Official Python-Objective-C bridge

### Development Tools

#### `pytest >= 7.0`
**Purpose:** Testing framework
**Why chosen:**
- Standard Python testing framework
- Rich plugin ecosystem
- Excellent fixtures and parametrization
- Easy to use

#### `pytest-benchmark >= 4.0`
**Purpose:** Performance benchmarking in tests
**Why chosen:**
- Integrates with pytest
- Statistical analysis
- Regression detection
- Comparative benchmarks

#### `pillow >= 10.0`
**Purpose:** Image manipulation and comparison in tests
**Why chosen:**
- Standard Python image library
- Required for visual comparison tests
- Format conversion
- Wide format support

### Optional/Utility

#### `opencv-python` (cv2)
**Purpose:** Image I/O in renderer save_image method
**Why chosen:**
- Fast image saving
- Wide format support
- Used in existing renderers
- Future: may migrate to Pillow for fewer dependencies

#### `loguru`
**Purpose:** Structured logging
**Why chosen:**
- Better API than stdlib logging
- Colored output
- Structured logging
- Used in renderer modules

## Dependency Selection Criteria

### Required Properties
1. **Well-maintained:** Active development, recent commits
2. **Popular:** >200 GitHub stars or widely used
3. **Documented:** Good API docs and examples
4. **Safe:** Minimal unsafe code, security audits
5. **Performant:** Benchmarked, no known performance issues

### Avoided Patterns
- Multiple dependencies for the same purpose
- Abandoned or unmaintained packages
- Dependencies with known security issues
- Packages that pull in excessive transitive dependencies
- Unstable or alpha-quality packages (except for new Rust crates)

## Dependency Audit

Last audited: 2024-11-13

**Process:**
1. Check for security advisories: `cargo audit`
2. Review transitive dependencies
3. Check for updates: `cargo outdated`
4. Review licensing (all MIT or Apache-2.0 compatible)

**Next audit:** Q1 2025

## Future Considerations

### Potential Removals
- `opencv-python` - migrate to Pillow for image saving
- Consider `fontdue` instead of `zeno` if quality improves

### Potential Additions
- `tiny-skia` - pure Rust alternative to Skia
- `rustybuzz` - pure Rust HarfBuzz alternative (when mature)
- `cosmic-text` - higher-level text layout (for future layout features)

### Platform-Specific
- DirectWrite bindings for Windows renderer (planned)
- iOS/tvOS support (via CoreText)
- Android support (via Skia or custom)

## Licensing

All dependencies use permissive licenses compatible with MIT/Apache-2.0:
- MIT: Most Rust crates
- Apache-2.0: Some Rust crates (dual-licensed)
- BSD: HarfBuzz, FreeType
- MPL-2.0: Some Mozilla components (compatible)

No GPL or AGPL dependencies (by design).
