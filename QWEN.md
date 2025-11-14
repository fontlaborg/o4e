---
this_file: CLAUDE.md
---

# o4e Development Guidelines

**Working on**: o4e (Open Font Engine) - /Users/adam/Developer/vcs/github.fontlaborg/o4e

## Project Mission

Provide **fast, cross-platform text rendering** with native platform backends. Nothing more, nothing less.

## Core Principles

1. **Simplicity First**: No fancy features. Core functionality only.
2. **Performance**: Fast rendering with native backends
3. **Cross-Platform**: macOS, Windows, Linux
4. **Dual API**: Rust library + Python bindings

## Architecture

```
o4e/
├── backends/o4e-core/      # Shared traits
├── backends/o4e-mac/       # CoreText (macOS)
├── backends/o4e-win/       # DirectWrite (Windows)
├── backends/o4e-icu-hb/    # ICU+HarfBuzz (cross-platform)
├── crates/o4e-render/      # SVG output
└── python/                 # PyO3 bindings
```

## What We DO

- ✅ Render text to PNG/SVG/raw RGBA
- ✅ Native backends (CoreText, DirectWrite, HarfBuzz)
- ✅ Load fonts (system, file, bytes)
- ✅ Unicode support (segmentation, shaping, bidi)
- ✅ Python bindings with simple API

## What We DON'T Do

- ❌ Fuzzing infrastructure
- ❌ Extensive benchmarking
- ❌ Color fonts (COLRv1/CPAL)
- ❌ Diagnostic tools
- ❌ Performance profiling
- ❌ Batch rendering analytics
- ❌ Path simplification
- ❌ Enterprise features

## Development Rules

1. **Code Quality**: Format with `cargo fmt`, check with `cargo clippy`
2. **Testing**: Basic unit tests + smoke tests
3. **Documentation**: Minimal inline comments explaining "why", not "what"
4. **Commits**: Use simple commit messages
5. **Dependencies**: Minimal, well-vetted only

## File Guidelines

Keep these files LEAN:
- `README.md` - Under 150 lines, usage-focused
- `PLAN.md` - Current refinement goals only
- `TODO.md` - Flat list of next tasks
- `WORK.md` - Current session notes only

## Testing Strategy

```bash
# Format and check
cargo fmt
cargo clippy --workspace --all-features

# Test
cargo test --workspace

# Build Python package
maturin build --release

# Test Python
pytest python/tests -v
```

## API Philosophy

**Rust**: Trait-based, backend-agnostic
```rust
let renderer = CoreTextBackend::new();
let output = renderer.render(&text, &font, &options)?;
```

**Python**: Simple and obvious
```python
from o4e import TextRenderer, Font
renderer = TextRenderer()
image = renderer.render("Hello", Font("Arial", 48))
```

## Performance Targets

- Single render: < 1ms typical
- Memory: < 50MB cache
- Startup: < 10ms first render

## When in Doubt

Ask: "Does this help render text faster or simpler?" If no, don't add it.
