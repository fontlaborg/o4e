---
this_file: CLAUDE.md
---

# Development Guidelines for o4e

This file provides guidance to Claude Code when working with code in this repository.

## Project Overview

**o4e** (open font renderer) is a text renderer development project that aims to provide a unified API for rendering text with fonts across multiple backend implementations (HarfBuzz + rasterizers, CoreText, DirectWrite). The project prioritizes CSS-familiar terminology with secondary support for HarfBuzz conventions.

**Key Components:**
- **Reference implementation (haforu)**: Rust-based high-performance batch font renderer in `reference/haforu/` with Python bindings via PyO3
- **API design documentation**: Comprehensive specification in `README.md` defining language-agnostic interfaces
- **Renderer prototypes**: Various rendering implementations in `reference/renderers/`

## Development Philosophy

### Chain-of-Thought First

Before any implementation, apply systematic reasoning:
- **Problem analysis**: What exactly are we solving and why?
- **Constraints**: What limitations must we respect?
- **Solution options**: What are 2–3 viable approaches with trade-offs?
- **Edge cases**: What could go wrong and how do we handle it?
- **Test strategy**: How will we verify this works correctly?

### Accuracy Over Agreement

- State confidence levels clearly: "I'm certain" vs "I believe" vs "This is an educated guess"
- If confidence is below 90%, use search tools (codebase, references, web)
- Challenge incorrect assumptions immediately
- Facts matter more than validation

### Simplicity and Verification

- **Build vs buy**: Always choose well-maintained packages over custom solutions
- **Verify, don't assume**: Test every function, every edge case
- **Complexity kills**: Every line of custom code is technical debt
- **Test or it doesn't exist**: Untested code is broken code

## Architecture

### Core Design Principles

1. **Language-agnostic API**: Designed to work consistently across Python, Rust, Swift, and JavaScript
2. **CSS-first terminology**: Prioritize CSS property names (`font-size`, `letter-spacing`) over native shaping library terms
3. **HarfBuzz compatibility**: Secondary support for HarfBuzz concepts (features, variations, clusters)
4. **Multiple output modes**: Support both raster images (PNG, PGM) and JSON shaping data

### Haforu Architecture (reference/haforu/)

**Rust Core Modules:**
- `batch.rs`: JSONL-based batch job processing
- `fonts.rs`: Memory-mapped font loading with LRU caching
- `shaping.rs`: Text shaping via HarfBuzz
- `render.rs`: Glyph rasterization using zeno
- `output.rs`: Image generation (PGM/PNG) and base64 encoding
- `error.rs`: Unified error handling
- `security.rs`: Font file validation and safety checks
- `input.rs`: Input validation and processing

**Python Bindings (python/):**
- Maturin for build/packaging
- PyO3 for Rust-Python bridge
- NumPy integration for array handling
- Batch and streaming APIs exposed

## Project Structure & Organization

Root-level specs live in `README.md`, the canonical API contract. Reference implementations stay under `reference/`: `haforu/` hosts the Rust crate plus Python bindings (`src/` for Rust, `python/haforu/` for the wheel, `python/tests/` for fixtures), while `renderers/` contains lightweight Python prototypes for CoreText, Skia, and HarfBuzz adapters. Leave `NEXTTASK.md` untouched; it tracks upstream priorities.

```
o4e/
├── README.md              # API specifications and design docs
├── NEXTTASK.md           # Current development focus (read-only)
├── CLAUDE.md             # This file
├── WORK.md               # Work progress updates
├── CHANGELOG.md          # Past change release notes
├── PLAN.md               # Detailed future goals
├── TODO.md               # Flat itemized task list
├── reference/
│   ├── haforu/           # Reference implementation
│   │   ├── src/          # Rust source
│   │   ├── python/       # Python bindings
│   │   ├── Cargo.toml    # Rust dependencies
│   │   └── pyproject.toml # Python package config
│   └── renderers/        # Prototype implementations
└── .git/
```

### File Path Tracking

**Mandatory**: Every source file must maintain a `this_file` record showing the path relative to project root.
- Place `this_file` near the top: as a comment after shebangs in code files, or in YAML frontmatter for markdown
- Update paths when moving files
- Omit leading `./`

## Development Workflow

### Before Starting Any Work

1. Read `WORK.md` for current progress and `CHANGELOG.md` for past changes
2. Read `README.md` to understand the project
3. Run existing tests to understand current state
4. Step back and think through the task step-by-step
5. Consider alternatives and choose the best option
6. Check for existing solutions in the codebase

### When Adding New Features

1. **Start with the API contract**: Update `README.md` specifications first
2. **Search for existing packages**: Check if this has been solved before
3. **Write tests first**: Define what success looks like
4. **Implement in Rust core**: Add to appropriate module in `src/`
5. **Expose to Python**: Update `src/python/mod.rs` and type stubs (`.pyi` files)
6. **Verify thoroughly**: Run all tests, check edge cases
7. **Update documentation**: Docstrings in Rust (`//!`) and Python

### Complexity Detection Triggers

**Rethink your approach immediately if you're:**
- Writing a utility function that feels "general purpose"
- Creating abstractions "for future flexibility"
- Adding error handling for errors that never happen
- Writing custom parsers, validators, or formatters
- Implementing caching, retry logic, or state management from scratch
- More than 3 levels of indentation
- Functions longer than 20 lines
- Files longer than 200 lines

## Commands & Tools

### Rust Development (haforu)

```bash
# Navigate to haforu directory first
cd reference/haforu

# Build and test
cargo build --release
cargo test
cargo fmt && cargo clippy --all-targets --all-features

# Run the CLI tool
cargo run --release -- [args]

# Build with Python bindings
cargo build --release --features python
```

### Python Development (haforu Python bindings)

```bash
cd reference/haforu

# Install in development mode
uv tool run maturin develop

# Or use maturin directly
maturin develop --features python
maturin build --release --features python

# Run Python tests
uvx hatch test
pytest python/tests/ -v

# Type checking
mypy python/haforu/
```

### Dependency Management

- Use `uv` for Python workflows: `uv add [package]`
- Use Cargo for Rust: standard `Cargo.toml` management
- **Never call `pip` directly**; use `uv pip install` if needed
- Keep dependencies managed via `uv add …`

## Testing Strategy

### Coverage Requirements

Target ≥80% coverage. Every function needs coverage.

**Rust Tests:**
- Unit tests embedded in each module (`mod tests` blocks at bottom of .rs files)
- Integration tests use `tempfile` and `insta` for snapshot testing
- Run all: `cargo test`
- Run specific: `cargo test fonts::tests`

**Python Tests:**
- Located in `python/tests/`
- Test files: `test_batch.py`, `test_streaming.py`, `test_numpy.py`, `test_errors.py`
- Uses pytest framework
- Run: `uvx hatch test` or `pytest python/tests/ -v`

### Testing Standards

- **Unit tests**: Every function gets at least one test
- **Edge cases**: Test empty glyph sets, invalid font paths, oversized bitmaps, none, negative numbers
- **Error cases**: Test what happens when things fail (network failures, missing files, bad permissions)
- **Integration**: Test that components work together
- **Test naming**: `test_function_name_when_condition_then_result`
- **Assert messages**: Always include helpful messages in assertions
- **Functional tests**: Maintain working examples in `reference/renderers/` that showcase realistic usage

### Verification Workflow (Mandatory)

1. Write the test first (define what success looks like)
2. Implement minimal code (just enough to pass)
3. Run the test
4. Test edge cases
5. Test error conditions
6. Document test results in `WORK.md`

## Coding Style & Standards

### Rust

- Follow Rust 2021 defaults with `rustfmt`
- Idiomatic module names (snake_case files, UpperCamelCase types, SCREAMING_SNAKE constants)
- Use `Utf8Path` (from `camino` crate) for cross-platform font path handling
- Error handling: `thiserror` for error types, `anyhow` for error context

### Python

- 4-space indents, full type hints
- `ruff` (line length 100)
- Docstrings that explain "what + why"
- PEP 8: Consistent formatting and naming
- PEP 20: Keep code simple & explicit, prioritize readability
- PEP 257: Write docstrings
- Use type hints in their simplest form (list, dict, | for unions)
- Modern code with `pathlib`
- Prefer explicit dataclasses/Pydantic models for structured data

### General Code Quality

- Use constants over magic numbers
- Write explanatory docstrings/comments that explain what and why
- Explain where and how code is used/referred to elsewhere
- Handle failures gracefully with retries, fallbacks, user guidance
- Address edge cases, validate assumptions, catch errors early
- Modularize repeated logic into concise, single-purpose functions
- Favor flat over nested structures

## Key Data Structures

### RenderRequest (API Contract)

The primary input structure for rendering operations:

```typescript
{
  text: string,                    // UTF-8 text to render
  font_input: FontInput,           // Font source specification
  text_style: TextStyle,           // Size, features, variations
  shaping?: ShapingOptions,        // Direction, script, language
  layout?: LayoutOptions,          // Canvas, alignment, cropping
  rendering?: RenderingOptions,    // Colors, antialiasing
  output: OutputOptions            // Format: image or JSON
}
```

### Haforu Job Spec (JSONL Batch Format)

```json
{
  "text": "Hello",
  "font": "path/to/font.ttf",
  "size": 100.0,
  "coords": {"wght": 600.0},       // Variable font axes
  "canvas_width": 3000,
  "canvas_height": 1200,
  "baseline_y": 0.0
}
```

### Shaping Output (hb-shape compatible)

```json
{
  "glyphs": [
    {
      "g": 42,                      // Glyph ID or name
      "cl": 0,                      // Cluster index
      "ax": 500,                    // X advance
      "ay": 0,                      // Y advance
      "dx": 0,                      // X offset
      "dy": 0,                      // Y offset
      "extents": {                  // Optional
        "x_bearing": 10,
        "y_bearing": -50,
        "width": 480,
        "height": 600
      }
    }
  ]
}
```

## Important Design Patterns

### Memory-Mapped Font Loading

Haforu uses `memmap2` for zero-copy font loading with an LRU cache (size: 512). Font instances are keyed by `(path, variation_coordinates)` to avoid redundant loading.

### Batch Processing

JSONL format allows processing multiple rendering jobs in a single invocation:
- One JSON object per line in input
- Output is JSONL with base64-encoded images or shaping data
- Enables parallel processing via `rayon`

### Error Handling

- Rust: Uses `thiserror` for error types, `anyhow` for error context
- Python: Maps Rust errors to Python exceptions via PyO3
- All errors include context about what failed (font path, text, etc.)

## Common Pitfalls

1. **Font path handling**: Always use `Utf8Path` (from `camino` crate) in Rust for cross-platform compatibility
2. **Coordinate systems**: Be careful with baseline positioning - different systems use different origins
3. **Variation axes**: Must be 4-character tags (e.g., "wght", "slnt")
4. **Feature tags**: OpenType features are also 4-character tags
5. **Memory limits**: Large batch jobs with many unique fonts can exhaust cache; monitor LRU size

## Security & Configuration

- Never commit proprietary fonts; keep local assets under an ignored `fonts/` folder
- Validate file I/O (see `reference/haforu/src/security.rs`)
- Store secrets in env vars and avoid hardcoded paths
- When updating renderer adapters, confirm platform dependencies (CoreText, Skia) exist before enabling via `is_available()`

## Commit & Pull Request Guidelines

Git history currently only contains the bootstrap commit, so set the bar:
- Write imperative, scoped messages (`Add haforu batch renderer bindings`)
- PRs should describe motivation, implementation notes, tests run, and link tracking issues or `NEXTTASK` items
- Include screenshots or sample CLI output when rendering differs visually
- Keep diffs focused—split protocol/spec edits from code changes
- Run `uvx hatch test` plus `cargo test` before opening a PR
- Document results in `WORK.md`

## Project Documentation to Maintain

- `README.md`: Purpose and functionality (canonical API contract)
- `CHANGELOG.md`: Past change release notes (accumulative)
- `PLAN.md`: Detailed future goals, clear plan that discusses specifics
- `TODO.md`: Flat simplified itemized `- []`-prefixed representation of `PLAN.md`
- `WORK.md`: Work progress updates including test results
- `DEPENDENCIES.md`: List of packages used and why each was chosen

## Special Commands

### `/test` Command

Run comprehensive tests and document results:

**For Rust:**
```bash
cd reference/haforu
cargo fmt && cargo clippy --all-targets --all-features
cargo test
```

**For Python:**
```bash
cd reference/haforu
uvx hatch test
```

Document all results in `WORK.md`.

### `/work` Command

1. Read `TODO.md` and `PLAN.md` files, think hard and reflect
2. Write down immediate items in this iteration into `WORK.md`
3. Write tests for the items first
4. Work on these items
5. Think, contemplate, research, reflect, refine, revise
6. Run the `/test` command tasks
7. Periodically remove completed items from `WORK.md`
8. Tick off completed items from `TODO.md` and `PLAN.md`
9. Update `WORK.md` with improvement tasks
10. Continue to the next item

### `/report` Command

1. Read `TODO.md` and `PLAN.md` files
2. Analyze recent changes
3. Run tests
4. Document changes in `CHANGELOG.md`
5. Remove completed items from `TODO.md` and `PLAN.md`

## Performance Considerations

- Haforu prioritizes throughput for batch operations
- Font loading uses memory mapping (zero-copy)
- Glyph rasterization uses `zeno` (fast software rasterizer)
- Parallel processing via `rayon` for batch jobs
- LRU cache (size: 512) for font instances

## Cross-Platform Notes

- **Font backends**: HarfBuzz works on all platforms; CoreText/DirectWrite are platform-specific
- **System fonts**: Require platform-specific font resolution
- **Image formats**: PNG requires `image` crate; PGM is text-based (simpler)

## Related Documentation

- HarfBuzz API: https://harfbuzz.github.io/
- CSS Fonts spec: https://www.w3.org/TR/css-fonts-4/
- OpenType spec: https://learn.microsoft.com/en-us/typography/opentype/spec/
- PyO3 guide: https://pyo3.rs/
- Maturin docs: https://www.maturin.rs/
