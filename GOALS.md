---
this_file: GOALS.md
---

# o4e: Open Font Engine - Project Goals & Vision

## Executive Summary

**o4e** is a high-performance, cross-platform text rendering metapackage that provides unified access to platform-native and cross-platform text rendering backends through Rust with Python bindings. The project delivers production-ready Unicode text processing, font shaping, and rendering to both vector (SVG) and raster formats.

## Core Mission

Build the fastest, most correct text rendering solution that:
1. **Leverages native platform capabilities** when available (CoreText on macOS, DirectWrite on Windows)
2. **Provides cross-platform fallbacks** using best-in-class libraries (ICU, HarfBuzz, FreeType)
3. **Outputs to multiple formats** including SVG for vector graphics and bitmaps for raster
4. **Maintains sub-millisecond performance** for single renders and high throughput for batch operations
5. **Exposes simple, consistent APIs** across all backends with CSS-first terminology

## Primary Objectives

### 1. Unicode Text Processing Excellence
- **Run segmentation**: Properly segment text into rendering runs based on:
  - Script boundaries (Latin → Arabic transitions)
  - Language changes (for proper shaping rules)
  - Font fallback requirements
  - Directional boundaries (LTR/RTL)
  - Style changes (bold, italic, etc.)
- **Normalization**: Handle NFC/NFD/NFKC/NFKD transformations
- **Bidirectional text**: Full UAX#9 compliance for mixed-direction text
- **Complex scripts**: Proper handling of Arabic, Indic, CJK, and other complex scripts

### 2. Font Shaping & Line Layout
- **OpenType feature support**: Full GSUB/GPOS processing
- **Variable fonts**: Complete axis interpolation
- **Line breaking**: UAX#14 compliant line breaking
- **Justification**: Full/partial justification with kashida
- **Vertical text**: Support for vertical writing modes
- **Font fallback**: Intelligent glyph substitution

### 3. High-Performance Rendering
- **Vector output (SVG)**:
  - Exact glyph outlines with proper winding
  - Cubic and quadratic Bézier support
  - Efficient path optimization
  - Color fonts (COLRv1, SVG-in-OT)
- **Raster output**:
  - Sub-pixel antialiasing
  - Hinting support (auto, slight, full)
  - Multiple bit depths (1, 2, 4, 8, 32)
  - Hardware acceleration where available

## Backend Architecture

### Tier 1: Platform-Native Backends (Highest Priority)

#### o4e-mac (CoreText Backend)
- **Platform**: macOS 11.0+
- **Technology**: Core Text + Core Graphics
- **Advantages**:
  - Native font matching and fallback
  - Hardware-accelerated rendering
  - Perfect system UI consistency
  - Color emoji support
- **Implementation**: Rust with objc2/metal crates

#### o4e-win (DirectWrite Backend)
- **Platform**: Windows 10+
- **Technology**: DirectWrite + Direct2D
- **Advantages**:
  - Native ClearType rendering
  - GPU acceleration via Direct2D
  - System font integration
  - Variable font support
- **Implementation**: Rust with windows-rs crate

### Tier 2: Cross-Platform Backends

#### o4e-icu-hb (ICU + HarfBuzz Backend)
- **Platform**: All platforms
- **Technology**: ICU4C + HarfBuzz + FreeType
- **Advantages**:
  - Consistent cross-platform behavior
  - Industry-standard shaping
  - Complete Unicode support
  - Reference implementation
- **Implementation**: Rust with FFI bindings

#### o4e-pure (Pure Rust Backend)
- **Platform**: All platforms including WASM
- **Technology**: rustybuzz + fontdue/ab_glyph
- **Advantages**:
  - No C dependencies
  - WASM-compatible
  - Embedded systems support
  - Smallest binary size
- **Implementation**: Pure Rust, no FFI

#### o4e-skia (Skia Backend)
- **Platform**: All platforms with GPU
- **Technology**: Skia (via rust-skia)
- **Advantages**:
  - GPU acceleration
  - Advanced effects (shadows, gradients)
  - PDF/XPS output
  - Used by Chrome/Flutter
- **Implementation**: Rust with skia-safe crate

## Performance Targets

### Single Render Performance
- **Simple Latin text (< 100 chars)**: < 0.5ms
- **Complex script (Arabic/Devanagari)**: < 2ms
- **CJK with fallback**: < 3ms
- **SVG output generation**: < 1ms overhead

### Batch Performance (5000+ renders)
- **Throughput**: > 10,000 renders/second
- **Memory**: < 100MB for font cache
- **Parallelization**: Linear scaling to 8 cores
- **Cache efficiency**: > 95% hit rate

### Startup Performance
- **First render**: < 10ms including font loading
- **Subsequent renders**: < 0.1ms with warm cache
- **Backend switching**: < 5ms

## Quality Metrics

### Correctness
- **Unicode conformance**: 100% UAX compliance
- **OpenType conformance**: Pass all HarfBuzz tests
- **Rendering accuracy**: Pixel-perfect match with native renderers
- **Complex script accuracy**: Match Uniscribe/CoreText output

### Developer Experience
- **API simplicity**: Single unified interface across all backends
- **Documentation**: Every public API documented with examples
- **Error handling**: Descriptive errors with recovery suggestions
- **Testing**: > 90% code coverage

## Python Package Structure

### Core Package: `o4e`
```python
pip install o4e  # Installs core with pure-Rust backend
```

### Optional Extras
```python
pip install o4e[mac]     # Adds CoreText backend
pip install o4e[windows] # Adds DirectWrite backend
pip install o4e[icu]     # Adds ICU+HarfBuzz backend
pip install o4e[skia]    # Adds Skia backend
pip install o4e[all]     # Installs all backends
pip install o4e[dev]     # Adds development tools
```

## Release Strategy

### Version 0.1.0 (MVP - 2 weeks)
- ✅ Basic haforu implementation (DONE)
- [ ] CoreText backend for macOS
- [ ] Python bindings with basic API
- [ ] SVG output support
- [ ] Basic documentation

### Version 0.2.0 (Cross-platform - 4 weeks)
- [ ] DirectWrite backend for Windows
- [ ] ICU+HarfBuzz backend
- [ ] Automatic backend selection
- [ ] Performance benchmarks

### Version 0.3.0 (Production - 6 weeks)
- [ ] Pure Rust backend
- [ ] Advanced line layout
- [ ] Font fallback chains
- [ ] Comprehensive test suite

### Version 1.0.0 (Stable - 8 weeks)
- [ ] All backends feature-complete
- [ ] Performance targets met
- [ ] Full documentation
- [ ] Security audit completed

## Success Criteria

### Technical Success
- **Performance**: Meet or exceed all performance targets
- **Compatibility**: Run on 95% of target platforms
- **Correctness**: Pass Unicode/OpenType conformance suites
- **Reliability**: < 1 crash per million renders

### Adoption Success
- **Users**: 1000+ downloads in first month
- **Contributors**: 5+ external contributors
- **Integration**: Used in 3+ production projects
- **Benchmarks**: Featured in font rendering benchmarks

## Non-Goals (Explicitly Out of Scope)

1. **Full typesetting engine**: We render lines, not pages
2. **Font editing**: We render fonts, not modify them
3. **Complex layout**: No CSS Grid/Flexbox (use higher-level libs)
4. **Font management**: No font installation/discovery
5. **Accessibility APIs**: Text rendering only, not screen readers

## Competitive Advantages

### vs. HarfBuzz
- **Native platform integration**: Use CoreText/DirectWrite when available
- **Built-in rendering**: Not just shaping but complete render pipeline
- **Better Python API**: Modern, Pythonic interface with type hints
- **SVG output**: Direct vector output without additional libraries

### vs. Skia
- **Lighter weight**: 10x smaller binary for text-only use cases
- **Faster startup**: No GPU context initialization required
- **Better text focus**: Optimized specifically for text, not general graphics
- **Simpler API**: Text-specific API vs. general canvas API

### vs. FreeType
- **Modern architecture**: Rust-based with memory safety
- **Better Unicode**: Integrated ICU for proper text segmentation
- **Platform native**: Uses CoreText/DirectWrite when available
- **Parallel processing**: Built for multi-core batch operations

## Development Principles

### Performance First
Every design decision prioritizes performance:
- Zero-copy where possible
- Memory-mapped fonts
- Lock-free concurrent access
- SIMD optimizations
- Minimal allocations

### Correctness Second
After performance, correctness is paramount:
- Comprehensive test coverage
- Fuzzing for security
- Unicode conformance
- Platform consistency

### Simplicity Third
API simplicity without sacrificing power:
- Sensible defaults
- Progressive disclosure
- Clear error messages
- Extensive examples

## Long-term Vision (Year 2+)

### Advanced Features
- **Smart caching**: ML-based render prediction
- **Cloud rendering**: Distributed rendering service
- **Format expansion**: Native PDF, PS, DOCX generation
- **Live preview**: Real-time rendering server

### Ecosystem Growth
- **Plugins**: Extensible effect system
- **Language bindings**: Go, Java, Swift, C#
- **Framework integration**: React, Flutter, Qt
- **Tool development**: Font inspector, render differ

### Research Areas
- **AI-assisted shaping**: Neural network-based complex script handling
- **Perceptual optimization**: Psychovisual rendering improvements
- **Compression**: Novel glyph compression algorithms
- **Energy efficiency**: Mobile/embedded optimizations

## Call to Action

o4e aims to be **the** definitive solution for high-performance text rendering. We're building:
1. The **fastest** text renderer through native platform integration
2. The **most correct** through comprehensive Unicode support
3. The **most portable** through multiple backend options
4. The **most accessible** through excellent Python bindings

Join us in making text rendering a solved problem.