# o4e Backend Architecture

This document describes the backend architecture of o4e and provides guidance on selecting and using different backends.

## Overview

o4e uses a trait-based architecture that allows different rendering backends to be swapped seamlessly. Each backend provides platform-specific optimizations while maintaining a unified API.

## Available Backends

### CoreText (macOS)

**Platform:** macOS 11.0+
**Feature flag:** `mac`
**Crate:** `o4e-mac`

CoreText is Apple's native text rendering framework, providing:
- Native macOS font rendering with pixel-perfect system consistency
- Full support for Apple's font collection and system fonts
- Hardware-accelerated rendering via Core Graphics
- Native color font support (Apple Color Emoji)
- Variable font axis support via `CTFont` descriptors

**Strengths:**
- Best performance on macOS (< 0.3ms for Latin text)
- Perfect integration with system fonts
- Supports all macOS-specific font features
- Native ClearType-equivalent antialiasing

**Limitations:**
- macOS only
- Requires `CoreFoundation` and `CoreText` frameworks
- May have different rendering than other platforms

**Usage:**
```python
from o4e import TextRenderer

# Explicit selection
renderer = TextRenderer(backend="coretext")

# Automatic on macOS
renderer = TextRenderer()  # Auto-selects CoreText on macOS
```

### DirectWrite (Windows)

**Platform:** Windows 10+
**Feature flag:** `windows`
**Crate:** `o4e-win`

DirectWrite is Microsoft's modern text rendering API, providing:
- Native Windows font rendering
- ClearType subpixel antialiasing
- Full DirectX integration
- Advanced typography features
- Variable font support via `IDWriteFontFace5`

**Strengths:**
- Best performance on Windows
- Native ClearType rendering
- Excellent variable font support
- Direct2D hardware acceleration available

**Limitations:**
- Windows only
- Requires Windows 10 or later
- COM initialization overhead

**Usage:**
```python
from o4e import TextRenderer

# Explicit selection
renderer = TextRenderer(backend="directwrite")

# Automatic on Windows
renderer = TextRenderer()  # Auto-selects DirectWrite on Windows
```

### ICU + HarfBuzz (Cross-platform)

**Platform:** All platforms
**Feature flag:** `icu`
**Crate:** `o4e-icu-hb`

The ICU+HarfBuzz backend uses industry-standard open-source libraries:
- **ICU** (International Components for Unicode) for text segmentation
- **HarfBuzz** for text shaping
- **ttf-parser** for font parsing
- **tiny-skia** for rasterization

**Strengths:**
- Works on all platforms (Linux, macOS, Windows, BSD)
- Excellent Unicode support
- Industry-standard shaping (same as Chrome, Firefox, Android)
- Predictable cross-platform rendering
- No platform dependencies

**Limitations:**
- Slightly slower than native backends (< 0.6ms vs < 0.3ms)
- May not match system font rendering exactly
- Requires font files (cannot use system font APIs directly)

**Usage:**
```python
from o4e import TextRenderer

# Explicit selection
renderer = TextRenderer(backend="harfbuzz")

# Automatic on Linux and non-mac/windows platforms
renderer = TextRenderer()  # Auto-selects HarfBuzz on Linux
```

## Backend Selection

### Automatic Selection

By default, o4e automatically selects the best backend for your platform:

```python
from o4e import TextRenderer

renderer = TextRenderer()  # Automatically selects:
# - CoreText on macOS
# - DirectWrite on Windows
# - ICU+HarfBuzz on Linux/other
```

### Manual Selection

You can explicitly choose a backend:

```python
# Force HarfBuzz backend (available on all platforms)
renderer = TextRenderer(backend="harfbuzz")

# Platform-specific backends
renderer = TextRenderer(backend="coretext")    # macOS only
renderer = TextRenderer(backend="directwrite")  # Windows only
```

### Python API

```python
from o4e import TextRenderer

# Check available backends
available = TextRenderer.list_backends()  # Returns: ["coretext", "harfbuzz"]

# Get the default backend for current platform
default = TextRenderer.get_default_backend()  # "coretext" on macOS

# Create with specific backend
renderer = TextRenderer(backend="harfbuzz")
```

## Backend Capabilities Comparison

| Feature | CoreText | DirectWrite | ICU+HarfBuzz |
|---------|----------|-------------|--------------|
| Platform | macOS | Windows | All |
| Performance | Excellent | Excellent | Good |
| Unicode Support | Excellent | Excellent | Excellent |
| Variable Fonts | Yes | Yes | Yes |
| Color Fonts | Yes | Yes | Yes (SVG) |
| Font Fallback | Automatic | Automatic | Manual |
| System Fonts | Native | Native | File-based |
| ClearType | N/A | Native | Emulated |
| Cross-platform | No | No | Yes |

## Font Loading

### System Fonts

System fonts work differently on each backend:

```python
from o4e import Font, TextRenderer

renderer = TextRenderer()

# CoreText and DirectWrite can use system font names
font = Font("Helvetica", size=48)  # Works on macOS
font = Font("Segoe UI", size=48)   # Works on Windows

# HarfBuzz requires font file paths
font = Font.from_path("/usr/share/fonts/truetype/noto/NotoSans-Regular.ttf", 48)
```

### Font Files

All backends support loading fonts from files:

```python
# Load from path
font = Font.from_path("/path/to/font.ttf", size=48)

# Load from memory
with open("font.ttf", "rb") as f:
    data = f.read()
font = Font.from_bytes("MyFont", data, size=48)
```

### Variable Fonts

All backends support variable fonts with axis values:

```python
font = Font(
    "Inter Variable",
    size=48,
    variations={
        "wght": 700,  # Weight axis
        "slnt": -10,  # Slant axis
        "wdth": 90,   # Width axis
    }
)
```

### OpenType Features

All backends support OpenType feature tags:

```python
font = Font(
    "MyFont",
    size=48,
    features={
        "kern": True,   # Kerning
        "liga": True,   # Ligatures
        "smcp": True,   # Small caps
        "swsh": False,  # Swashes disabled
    }
)
```

## Font Fallback

### CoreText

CoreText automatically handles font fallback:
- Uses system font cascade lists
- Automatically finds glyphs in fallback fonts
- Matches system text rendering behavior

### DirectWrite

DirectWrite provides automatic fallback:
- Uses system font fallback chains
- Locale-aware fallback selection
- System consistency

### ICU+HarfBuzz

HarfBuzz requires explicit fallback configuration:

```python
# o4e provides automatic fallback for common scripts
renderer = TextRenderer(backend="harfbuzz")

# The backend will automatically try:
# 1. The specified font
# 2. Noto Sans for Latin
# 3. Noto Sans CJK for CJK scripts
# 4. Noto Naskh Arabic for Arabic
# 5. etc.
```

You can customize fallback by providing font search directories:

```bash
# Set font search directories via environment variable
export O4E_FONT_DIRS="/usr/share/fonts:/usr/local/share/fonts:$HOME/.fonts"
```

## Performance Tuning

### Caching

All backends use multi-level caching:

```python
renderer = TextRenderer()

# Cache is automatic, but you can clear it if needed
renderer.clear_cache()
```

### Batch Processing

For maximum performance, use batch rendering:

```python
items = [
    {"text": f"Item {i}", "font": Font("Arial", 24)}
    for i in range(1000)
]

# Renders in parallel across all CPU cores
results = renderer.render_batch(items, format="png")
```

### Backend-Specific Optimizations

**CoreText:**
- Keep fonts alive to avoid descriptor recreation
- Use font variations instead of creating new fonts

**DirectWrite:**
- Reuse render targets when possible
- Enable ClearType for better readability

**ICU+HarfBuzz:**
- Preload fonts at startup
- Use glyph caching for repeated renders

## Thread Safety

All backends are thread-safe:
- `TextRenderer` instances can be shared across threads
- Caches use lock-free data structures where possible
- Batch rendering uses `rayon` for parallel execution

```python
from concurrent.futures import ThreadPoolExecutor

renderer = TextRenderer()
font = Font("Arial", 24)

def render_text(text):
    return renderer.render(text, font)

with ThreadPoolExecutor(max_workers=8) as executor:
    results = list(executor.map(render_text, texts))
```

## Troubleshooting

### Backend Not Available

```python
try:
    renderer = TextRenderer(backend="coretext")
except RuntimeError as e:
    print(f"Backend not available: {e}")
    # Fall back to HarfBuzz
    renderer = TextRenderer(backend="harfbuzz")
```

### Font Not Found

```python
try:
    font = Font("NonexistentFont", 24)
    renderer.render("Test", font)
except RuntimeError as e:
    print(f"Font error: {e}")
    # Use a fallback font
    font = Font.from_path("/path/to/fallback.ttf", 24)
```

### Poor Performance

1. Use batch rendering for multiple texts
2. Avoid creating new Font objects repeatedly
3. Clear cache if memory usage is too high
4. Consider using a faster backend for your platform

## Building Custom Backends

To add a new backend, implement the `Backend` trait:

```rust
use o4e_core::{Backend, Result, TextRun, Font, ShapingResult, RenderOutput};

struct MyBackend;

impl Backend for MyBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        // Implement text segmentation
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // Implement text shaping
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // Implement rendering
    }

    fn name(&self) -> &str {
        "mybackend"
    }

    fn clear_cache(&self) {
        // Clear any caches
    }
}
```

See `backends/o4e-core/src/traits.rs` for the complete trait definition.

## See Also

- [README.md](../README.md) - Quick start and installation
- [PLAN.md](../PLAN.md) - Implementation roadmap
- [GOALS.md](../GOALS.md) - Project goals and vision
