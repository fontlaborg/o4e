---
this_file: README.md
---

# o4e - Open Font Engine

High-performance, multi-backend text rendering for Python and Rust with native platform integration.

## Features

- 🚀 **Multiple Backends**: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (cross-platform)
- 📝 **Complete Unicode**: Proper text segmentation, bidirectional support, complex scripts
- 🎨 **Multiple Outputs**: SVG vectors, PNG/JPEG rasters, raw pixel buffers
- ⚡ **Blazing Fast**: < 0.5ms single render, > 10,000 renders/sec batch mode
- 🔧 **Simple API**: Unified interface across all backends with CSS-style properties
- 🐍 **Python + Rust**: Fast Rust core with ergonomic Python bindings

## Installation

```bash
# Core package with pure-Rust backend
pip install o4e

# Platform-specific backends
pip install o4e[mac]      # CoreText for macOS
pip install o4e[windows]  # DirectWrite for Windows
pip install o4e[icu]      # ICU+HarfBuzz cross-platform
pip install o4e[skia]     # Skia GPU-accelerated
pip install o4e[all]      # All backends
```

## Quick Start

```python
from o4e import TextRenderer, Font

# Automatic backend selection based on platform
renderer = TextRenderer()

# Render text to PNG
image = renderer.render(
    text="Hello, 世界! مرحبا",
    font=Font("Arial", size=48),
    output_format="png"
)
image.save("hello.png")

# Render to SVG for vector output
svg = renderer.render(
    text="Scalable Text",
    font=Font("Helvetica", size=72, weight=700),
    output_format="svg"
)
with open("scalable.svg", "w") as f:
    f.write(svg)

# Get shaping information (like hb-shape)
shaping = renderer.shape(
    text="Complex اردو Text",
    font=Font("Noto Sans", size=36),
    features={"kern": True, "liga": True}
)
for glyph in shaping.glyphs:
    print(f"Glyph {glyph.id}: advance={glyph.advance}, cluster={glyph.cluster}")
```

## Advanced Usage

### Specifying Backends

```python
from o4e import TextRenderer
from o4e.backends import CoreTextBackend, DirectWriteBackend, HarfBuzzBackend

# Use platform-native backend
if sys.platform == "darwin":
    renderer = TextRenderer(backend=CoreTextBackend())
elif sys.platform == "win32":
    renderer = TextRenderer(backend=DirectWriteBackend())
else:
    renderer = TextRenderer(backend=HarfBuzzBackend())
```

### Unicode Text Segmentation

```python
from o4e import TextSegmenter

# Segment text into rendering runs
segmenter = TextSegmenter()
runs = segmenter.segment(
    "Hello 世界 مرحبا Здравствуй",
    font_fallback=True,  # Split by font coverage
    script_itemize=True,  # Split by script
    bidi_resolve=True     # Split by direction
)

for run in runs:
    print(f"Run: '{run.text}' script={run.script} direction={run.direction}")
```

### Variable Fonts

```python
# Control variable font axes
font = Font(
    "Inter Variable",
    size=48,
    variations={"wght": 700, "slnt": -10}  # Weight=700, Slant=-10
)

image = renderer.render("Variable Typography", font=font)
```

### Batch Rendering

```python
# Efficient batch processing
batch = [
    {"text": "First", "font": Font("Arial", 24)},
    {"text": "Second", "font": Font("Times", 32)},
    {"text": "Third", "font": Font("Courier", 28)},
]

# Renders in parallel using all CPU cores
results = renderer.render_batch(batch, output_format="png")
```

### SVG Output with Paths

```python
# Get exact glyph outlines as SVG
svg_data = renderer.render(
    text="Vector",
    font=Font("Helvetica", 100),
    output_format="svg",
    svg_options={
        "include_paths": True,  # Include glyph path data
        "simplify": True,       # Optimize paths
        "precision": 2          # Decimal precision
    }
)
```

## API Reference

### Core Classes

#### `TextRenderer`
Main renderer class that manages backends and rendering operations.

```python
TextRenderer(backend=None, cache_size=512, parallel=True)
```

- `backend`: Specific backend to use (auto-detected if None)
- `cache_size`: Number of fonts to cache
- `parallel`: Enable parallel processing for batches

#### `Font`
Font specification with CSS-style properties.

```python
Font(family, size, weight=400, style="normal", variations=None, features=None)
```

- `family`: Font family name or path to font file
- `size`: Font size in pixels
- `weight`: Font weight (100-900)
- `style`: "normal", "italic", or "oblique"
- `variations`: Dict of variation axes for variable fonts
- `features`: Dict of OpenType features

#### `TextSegmenter`
Unicode-aware text segmentation for complex text.

```python
TextSegmenter(locale="en-US", line_break=True, word_break=True)
```

### Rendering Methods

#### `render()`
Render text to specified format.

```python
render(text, font, output_format="png", **options) -> Union[bytes, str, Image]
```

Supported formats:
- `"png"`, `"jpeg"`, `"webp"`: Raster formats (returns PIL Image)
- `"svg"`: Vector format (returns SVG string)
- `"raw"`: Raw pixel data (returns numpy array)
- `"pdf"`: PDF document (returns bytes)

#### `shape()`
Get shaping information without rendering.

```python
shape(text, font, **options) -> ShapingResult
```

Returns glyph IDs, positions, advances, and clusters.

#### `render_batch()`
Efficiently render multiple texts.

```python
render_batch(items, output_format="png", max_workers=None) -> List
```

### Options

#### Rendering Options
```python
{
    "color": "#000000",           # Text color (hex or rgb())
    "background": "transparent",   # Background color
    "antialias": "subpixel",      # none|grayscale|subpixel
    "hinting": "slight",          # none|slight|full
    "dpi": 72,                    # DPI for scaling
    "padding": 10,                # Padding around text
    "line_height": 1.2,          # Line height multiplier
}
```

#### Shaping Options
```python
{
    "direction": "auto",          # ltr|rtl|auto
    "language": "en",            # BCP-47 language tag
    "script": "auto",            # ISO 15924 script code
    "features": {                # OpenType features
        "kern": True,
        "liga": True,
        "smcp": False
    }
}
```

## Performance

| Operation | Time | Throughput |
|-----------|------|------------|
| Simple Latin text | < 0.5ms | 2,000/sec |
| Complex Arabic | < 2ms | 500/sec |
| CJK with fallback | < 3ms | 333/sec |
| Batch (1000 items) | < 100ms | 10,000/sec |
| SVG generation | < 1ms | 1,000/sec |

## Platform Support

| Platform | Backends | Status |
|----------|----------|--------|
| macOS 11+ | CoreText, HarfBuzz, Skia | ✅ Full support |
| Windows 10+ | DirectWrite, HarfBuzz, Skia | ✅ Full support |
| Linux | HarfBuzz, Skia | ✅ Full support |
| WASM | Pure Rust | 🚧 Beta |
| iOS/Android | CoreText/Skia | 📋 Planned |

## Development

See [GOALS.md](GOALS.md) for project vision and [PLAN.md](PLAN.md) for implementation details.

### Building from Source

```bash
# Clone repository
git clone https://github.com/fontlaborg/o4e
cd o4e

# Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build all backends
cargo build --release --all-features

# Build Python package
maturin develop --release

# Run tests
cargo test
pytest tests/
```

### Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

## License

MIT OR Apache-2.0 (dual-licensed)

## Acknowledgments

o4e builds on the shoulders of giants:
- [HarfBuzz](https://harfbuzz.github.io/) for text shaping
- [ICU](https://icu.unicode.org/) for Unicode support
- [FreeType](https://freetype.org/) for font rendering
- [Skia](https://skia.org/) for GPU acceleration

## Citation

```bibtex
@software{o4e2024,
  title = {o4e: Open Font Engine},
  author = {Font Laboratory Contributors},
  year = {2024},
  url = {https://github.com/fontlaborg/o4e}
}
```