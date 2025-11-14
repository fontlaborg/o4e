---
this_file: README.md
---

# o4e - Open Font Engine

**Fast, cross-platform text rendering using native platform APIs.**

## What It Does

o4e renders text to images (PNG/SVG) using platform-native text engines:
- **macOS**: CoreText
- **Windows**: DirectWrite
- **Linux**: ICU + HarfBuzz

## Why o4e?

1. **Native Performance**: Uses CoreText/DirectWrite for best platform performance
2. **Cross-Platform**: Same API works everywhere
3. **Python + Rust**: Fast Rust core, ergonomic Python bindings
4. **Simple**: One purpose—render text fast

## Installation

```bash
pip install o4e
```

## Quick Start

```python
from o4e import TextRenderer, Font

# Create renderer (auto-selects best backend)
renderer = TextRenderer()

# Render to PNG
png_bytes = renderer.render(
    "Hello, World!",
    Font("Arial", 48),
    format="png"
)

# Save
with open("output.png", "wb") as f:
    f.write(png_bytes)
```

## API

### TextRenderer()

Auto-selects backend:
- macOS → CoreText
- Windows → DirectWrite
- Linux → ICU+HarfBuzz

**Methods**:
- `render(text, font, format="png")` → bytes
- `shape(text, font)` → ShapingResult
- `clear_cache()` → None

### Font(family, size, weight=400, style="normal")

Font specification.

**Class methods**:
- `Font.from_path(path, size)` - Load from file
- `Font.from_bytes(name, data, size)` - Load from memory

### Formats

- `"png"` - PNG image
- `"svg"` - SVG vector
- `"raw"` - RGBA pixel data

## Examples

### Custom Font

```python
# From file
font = Font.from_path("/path/to/font.ttf", 36)
image = renderer.render("Text", font)

# From memory
with open("font.ttf", "rb") as f:
    data = f.read()
font = Font.from_bytes("MyFont", data, 36)
```

### Get Shaping Info

```python
shaping = renderer.shape("Hello", Font("Arial", 24))
for glyph in shaping.glyphs:
    print(f"Glyph {glyph.id}: x={glyph.x}, advance={glyph.advance}")
```

### SVG Output

```python
svg = renderer.render("Vector Text", Font("Helvetica", 64), format="svg")
print(svg)  # SVG XML string
```

## How It Works

```
Text + Font → Platform Backend → Pixels/SVG
              (CoreText/DirectWrite/HarfBuzz)
```

1. **Segmentation**: Break text into runs by script/direction
2. **Shaping**: Convert characters to positioned glyphs
3. **Rendering**: Draw glyphs to bitmap or extract paths

## Platform Support

| Platform | Backend | Status |
|----------|---------|--------|
| macOS 11+ | CoreText | ✅ |
| Windows 10+ | DirectWrite | ✅ |
| Linux | ICU+HarfBuzz | ✅ |

## Building from Source

```bash
# Clone
git clone https://github.com/fontlaborg/o4e
cd o4e

# Build Rust
cargo build --release

# Build Python wheel
pip install maturin
maturin build --release

# Install
pip install target/wheels/*.whl
```

## Testing

```bash
# All tests
cargo test --workspace
pytest python/tests -v

# Quick check
./test.sh
```

## License

MIT OR Apache-2.0

## Credits

Built with:
- [HarfBuzz](https://harfbuzz.github.io/) - Text shaping
- [ICU](https://icu.unicode.org/) - Unicode
- [ttf-parser](https://github.com/RazrFalcon/ttf-parser) - Font parsing
- [tiny-skia](https://github.com/RazrFalcon/tiny-skia) - Rendering
