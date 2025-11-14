---
this_file: PLAN.md
---

# o4e Refinement Plan

## Mission

**Fast, simple, cross-platform text rendering**. Nothing more.

## Current State

✅ **Working**:
- 3 backends: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (Linux)
- Python bindings with auto backend selection
- PNG, SVG, raw RGBA output
- Font loading: system, file, bytes
- Unicode: segmentation, shaping, bidi
- Basic caching

✅ **Tests passing**: 13/13

## Refinement Goals

### 1. Code Simplification
- Remove fuzzing infrastructure (fuzz/ directory, .github/workflows/fuzz.yml)
- Remove diagnostics module (backends/o4e-core/src/diagnostics.rs)
- Remove benchmarking harness (benches/ directory)
- Remove color font support code (COLRv1/CPAL in svg.rs)
- Simplify RenderOptions (remove rarely-used options)

### 2. Performance
- Verify cache effectiveness
- Check memory usage (target: < 50MB)
- Measure typical render times (target: < 1ms)

### 3. API Polish
- Ensure Python API is minimal and obvious
- Remove unused Rust public APIs
- Simplify error messages

### 4. Documentation
- Keep README.md under 150 lines
- Remove verbose inline comments
- Update examples to be minimal

### 5. Packaging
- Verify wheels build correctly on all platforms
- Test installation process
- Ensure minimal dependencies

## Non-Goals

These will NOT be implemented:
- Fuzzing infrastructure
- Performance benchmarking suite
- Diagnostic tools
- Batch rendering analytics
- Color font support
- Path simplification algorithms
- Enterprise features
- Configuration systems

## Success Criteria

- ✅ All tests pass
- ✅ Python API is < 100 lines of meaningful code
- ✅ README.md is < 150 lines
- ✅ No clippy warnings
- ✅ Renders text fast (< 1ms typical)
- ✅ Works on macOS, Windows, Linux

## Next Actions

1. Remove bloat (fuzz/, benches/, diagnostics)
2. Verify performance targets
3. Test on all platforms
4. Tag v0.1.0 release
