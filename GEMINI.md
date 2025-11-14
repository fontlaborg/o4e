---
this_file: CLAUDE.md
---

# Development Guidelines for o4e

This file provides guidance to Claude Code when working with the o4e multi-backend text rendering engine.

IMPORTANT: When you’re working, REGULARLY remind me & yourself which folder you’re working in and what project you’re working on. 

## Project Overview

**o4e** (Open Font Engine) is a high-performance text rendering metapackage providing:
- Multiple Rust backends: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (cross-platform)
- Complete Unicode text processing: segmentation, bidirectional text, complex scripts
- Multiple output formats: SVG vectors, raster bitmaps, shaping data
- Python bindings with optional extras for each backend

## Critical Architecture Decisions

### Workspace Structure
```
o4e/                           # Root workspace
├── Cargo.toml                 # Workspace definition
├── backends/
│   ├── o4e-core/             # Shared traits and utilities
│   ├── o4e-mac/              # CoreText backend
│   ├── o4e-win/              # DirectWrite backend
│   ├── o4e-icu-hb/           # ICU+HarfBuzz backend
│   ├── o4e-pure/             # Pure Rust backend
│   └── o4e-skia/             # Skia backend
├── crates/
│   ├── o4e-api/              # Public API types
│   ├── o4e-unicode/          # Unicode segmentation
│   ├── o4e-shaping/          # Shaping abstraction
│   └── o4e-render/           # Rendering abstraction
├── python/
│   ├── src/                  # PyO3 bindings
│   ├── o4e/                  # Python package
│   └── tests/                # Python tests
└── examples/                  # Working examples
```

### Development Principles

#### 1. RAPID DEVELOPMENT FIRST
- **MVP in 2 weeks**: Focus on working code over perfect code
- **Iterate fast**: Ship early, refine later
- **70% reuse**: Maximize code sharing across backends
- **No premature optimization**: Profile first, optimize second

#### 2. Backend Independence
- Each backend is a separate crate with minimal dependencies
- Shared functionality goes in `o4e-core`
- Platform-specific code is isolated
- Feature flags control compilation

#### 3. Performance Targets
- Single render: < 0.5ms for Latin, < 2ms for complex scripts
- Batch: > 10,000 renders/second
- Memory: < 100MB font cache
- Startup: < 10ms first render

## Implementation Strategy

### Phase 1: Core Infrastructure (Week 1)
1. **Workspace setup**: Multi-crate Cargo workspace
2. **Trait definitions**: Backend, Segmenter, Shaper, Renderer traits
3. **Shared utilities**: Font loading, caching, error handling
4. **API types**: Font, TextRun, ShapingResult, RenderOptions

### Phase 2: First Backend (Week 1-2)
1. **o4e-mac**: CoreText backend for macOS
   - Use `objc2` and `core-foundation` crates
   - Implement traits from o4e-core
   - Focus on correctness over optimization
2. **Python bindings**: Basic PyO3 wrapper
   - Simple API: `render(text, font) -> Image`
   - Automatic backend selection

### Phase 3: Cross-Platform (Week 2-3)
1. **o4e-icu-hb**: ICU+HarfBuzz backend
   - Use existing `harfbuzz_rs` bindings
   - ICU for segmentation via `icu_segmenter`
   - FreeType for rasterization
2. **o4e-win**: DirectWrite backend
   - Use `windows-rs` crate
   - Focus on ClearType rendering

### Phase 4: Advanced Features (Week 3-4)
1. **SVG output**: Glyph outline extraction
2. **Batch processing**: Parallel rendering with `rayon`
3. **Font fallback**: Automatic font selection
4. **Performance optimization**: Profiling and tuning

## Key Technical Patterns

### Trait-Based Architecture
```rust
// o4e-core/src/traits.rs
pub trait Backend: Send + Sync {
    fn segment(&self, text: &str) -> Vec<TextRun>;
    fn shape(&self, run: &TextRun, font: &Font) -> ShapingResult;
    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> RenderOutput;
}

pub trait TextSegmenter {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Vec<TextRun>;
}

pub trait FontShaper {
    fn shape(&self, text: &str, font: &Font, features: &Features) -> ShapingResult;
}

pub trait GlyphRenderer {
    fn render_to_bitmap(&self, glyphs: &[Glyph]) -> Bitmap;
    fn render_to_svg(&self, glyphs: &[Glyph]) -> String;
}
```

### Zero-Copy Font Loading
```rust
// Use memory-mapped fonts for performance
use memmap2::Mmap;
use std::sync::Arc;

pub struct FontData {
    mmap: Arc<Mmap>,
    face_index: u32,
}
```

### Efficient Caching
```rust
// LRU cache for shaped results
use lru::LruCache;
use std::sync::Mutex;

pub struct ShapeCache {
    cache: Mutex<LruCache<ShapeCacheKey, Arc<ShapingResult>>>,
}
```

### Platform Abstraction
```rust
// Conditional compilation for backends
#[cfg(target_os = "macos")]
pub use o4e_mac::CoreTextBackend as DefaultBackend;

#[cfg(target_os = "windows")]
pub use o4e_win::DirectWriteBackend as DefaultBackend;

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub use o4e_icu_hb::HarfBuzzBackend as DefaultBackend;
```

## Python API Design

### Simple by Default
```python
# Simplest possible API
from o4e import render
image = render("Hello World", font="Arial", size=48)
```

### Progressive Complexity
```python
# More control when needed
from o4e import TextRenderer, Font

renderer = TextRenderer(backend="coretext", cache_size=1024)
font = Font("Inter", size=36, variations={"wght": 700})
image = renderer.render("Text", font=font, color="#FF0000")
```

### Type Safety
```python
# Use type hints everywhere
from typing import Optional, Dict, Union
from pathlib import Path

def render(
    text: str,
    font: Union[str, Path, Font],
    size: Optional[float] = None,
    **options: Dict[str, Any]
) -> Image:
    ...
```

## Build System

### Cargo Workspace
```toml
# Root Cargo.toml
[workspace]
members = [
    "backends/*",
    "crates/*",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
authors = ["Font Laboratory"]
license = "MIT OR Apache-2.0"

[workspace.dependencies]
# Shared dependencies
thiserror = "1.0"
anyhow = "1.0"
log = "0.4"
rayon = "1.10"
lru = "0.12"
```

### Maturin Configuration
```toml
# pyproject.toml
[build-system]
requires = ["maturin>=1.0,<2.0"]
build-backend = "maturin"

[project]
name = "o4e"
version = "0.1.0"
requires-python = ">=3.8"

[project.optional-dependencies]
mac = ["o4e[coretext]"]
windows = ["o4e[directwrite]"]
icu = ["o4e[harfbuzz]"]
all = ["o4e[mac,windows,icu,skia]"]
```

### GitHub Actions
```yaml
# .github/workflows/release.yml
name: Release
on:
  push:
    tags:
      - 'v*'

jobs:
  build-wheels:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: PyO3/maturin-action@v1
        with:
          command: build
          args: --release
      - uses: actions/upload-artifact@v3
```

## Testing Strategy

### Unit Tests
- Each backend has comprehensive unit tests
- Test Unicode edge cases: RTL, combining marks, emoji
- Test performance: ensure targets are met
- Test correctness: compare with reference renderers

### Integration Tests
- Cross-backend consistency tests
- Python binding tests
- Performance benchmarks
- Visual regression tests

### Example Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_latin_render() {
        let backend = DefaultBackend::new();
        let result = backend.render("Hello", &font, &options);
        assert!(result.width > 0);
        assert!(result.height > 0);
    }

    #[test]
    fn test_arabic_shaping() {
        let backend = DefaultBackend::new();
        let shaped = backend.shape("مرحبا", &font);
        assert_eq!(shaped.glyphs.len(), 5);
    }

    #[test]
    fn test_performance() {
        let start = Instant::now();
        for _ in 0..1000 {
            backend.render("Test", &font, &options);
        }
        assert!(start.elapsed() < Duration::from_secs(1));
    }
}
```

## Common Commands

### Development
```bash
# Build all backends
cargo build --all-features

# Test specific backend
cargo test -p o4e-mac

# Build Python package
maturin develop --features python

# Run Python tests
pytest python/tests/ -v

# Benchmark performance
cargo bench

# Check code quality
cargo clippy --all-targets --all-features
cargo fmt --check
```

### Release
```bash
# Create release tag
git tag v0.1.0
git push origin v0.1.0

# Build wheels for PyPI
maturin build --release

# Upload to PyPI
maturin upload
```

## Performance Optimization Checklist

1. **Memory-mapped fonts**: Never load fonts into memory
2. **LRU caching**: Cache shaped text and rendered glyphs
3. **Parallel processing**: Use rayon for batch operations
4. **SIMD when available**: Use std::simd for pixel operations
5. **Zero allocations in hot path**: Pre-allocate buffers
6. **Profile before optimizing**: Use cargo-flamegraph

## Security Considerations

1. **Font validation**: Validate font files before loading
2. **Path traversal**: Sanitize all file paths
3. **Memory limits**: Cap cache sizes and buffer allocations
4. **Timeout handling**: Limit rendering time for complex text
5. **Fuzzing**: Use cargo-fuzz for security testing

## Debugging Tips

1. **Enable logging**: `RUST_LOG=debug cargo run`
2. **Visual debugging**: Save intermediate bitmaps
3. **Performance profiling**: Use perf/Instruments/VTune
4. **Memory debugging**: Use valgrind/AddressSanitizer
5. **Cross-platform testing**: Use CI for all platforms

## Code Review Checklist

Before submitting PRs:
- [ ] Tests pass on all platforms
- [ ] Performance targets met
- [ ] Documentation updated
- [ ] Examples work
- [ ] No unsafe code without justification
- [ ] Error messages are helpful
- [ ] Code follows Rust idioms

## Rapid Development Rules

1. **Ship working code first**: Perfection comes in v2
2. **Measure before optimizing**: Profile actual bottlenecks
3. **Reuse existing code**: 70% of code is shared
4. **Automate everything**: CI/CD handles releases
5. **Document as you go**: Comments explain "why"
6. **Test the critical path**: 80/20 rule for coverage

## Getting Help

- HarfBuzz docs: https://harfbuzz.github.io/
- CoreText: https://developer.apple.com/documentation/coretext
- DirectWrite: https://docs.microsoft.com/en-us/windows/win32/directwrite/
- ICU: https://unicode-org.github.io/icu/
- Skia: https://skia.org/docs/

## Next Actions

1. Set up Cargo workspace structure
2. Define core traits in o4e-core
3. Implement CoreText backend (o4e-mac)
4. Create minimal Python bindings
5. Add ICU+HarfBuzz backend
6. Implement SVG output
7. Add DirectWrite backend
8. Optimize performance
9. Release v0.1.0