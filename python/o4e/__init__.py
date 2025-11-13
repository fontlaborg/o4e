# this_file: python/o4e/__init__.py

"""o4e - Open Font Engine

High-performance, multi-backend text rendering for Python.

Features:
- Multiple backends: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (cross-platform)
- Output formats: PNG, SVG, raw bitmap data
- Full Unicode support with complex script shaping
- Batch processing with parallelization
- PIL/Pillow and NumPy integration
"""

from typing import Optional, Union, Dict, Any, List, Tuple, BinaryIO
from pathlib import Path
from enum import Enum
import io
import warnings

try:
    from . import o4e as _native
    _TextRenderer = _native.TextRenderer
    _Font = _native.Font
    get_version = _native.get_version
    ShapingResult = _native.ShapingResult
    Glyph = _native.Glyph
except ImportError:
    # Try direct import
    try:
        import o4e as _native
        _TextRenderer = _native.TextRenderer
        _Font = _native.Font
        get_version = _native.get_version
        ShapingResult = _native.ShapingResult
        Glyph = _native.Glyph
    except ImportError:
        # Fallback for development/testing without compiled module
        _TextRenderer = None
        _Font = None
        ShapingResult = None
        Glyph = None
        def get_version():
            return "0.1.0-dev"

# Optional dependencies
try:
    from PIL import Image as PILImage
    HAS_PIL = True
except ImportError:
    PILImage = None
    HAS_PIL = False

try:
    import numpy as np
    HAS_NUMPY = True
except ImportError:
    np = None
    HAS_NUMPY = False

__version__ = get_version()
__all__ = [
    "TextRenderer", "Font", "render", "render_to_file", "shape_text",
    "list_backends", "get_default_backend", "RenderFormat", "Direction",
    "ShapingResult", "Glyph", "Bitmap", "__version__"
]


class RenderFormat(Enum):
    """Supported output formats for rendering."""
    RAW = "raw"      # Raw RGBA bitmap data
    PNG = "png"      # PNG image bytes
    SVG = "svg"      # SVG XML string


class Direction(Enum):
    """Text direction for bidirectional text."""
    LEFT_TO_RIGHT = "ltr"
    RIGHT_TO_LEFT = "rtl"
    AUTO = "auto"


class Bitmap:
    """Raw bitmap data with metadata."""

    def __init__(self, data: bytes, width: int, height: int, format: str = "rgba"):
        """Create a bitmap from raw data.

        Args:
            data: Raw pixel data
            width: Width in pixels
            height: Height in pixels
            format: Pixel format ("rgba", "bgra", "rgb", "gray")
        """
        self.data = data
        self.width = width
        self.height = height
        self.format = format

    def to_numpy(self) -> 'np.ndarray':
        """Convert to numpy array.

        Returns:
            NumPy array of shape (height, width, channels)
        """
        if not HAS_NUMPY:
            raise ImportError("NumPy is required for this operation. Install with: pip install numpy")

        channels = 4 if self.format in ("rgba", "bgra") else 3 if self.format == "rgb" else 1
        arr = np.frombuffer(self.data, dtype=np.uint8)
        return arr.reshape((self.height, self.width, channels))

    def to_pil(self) -> 'PILImage.Image':
        """Convert to PIL/Pillow Image.

        Returns:
            PIL Image object
        """
        if not HAS_PIL:
            raise ImportError("PIL/Pillow is required for this operation. Install with: pip install Pillow")

        mode = "RGBA" if self.format == "rgba" else "RGB" if self.format == "rgb" else "L"
        return PILImage.frombytes(mode, (self.width, self.height), self.data)

    def save(self, path: Union[str, Path], format: Optional[str] = None):
        """Save bitmap to file.

        Args:
            path: Output file path
            format: Image format (auto-detected from extension if None)
        """
        if not HAS_PIL:
            # Fallback to saving raw data
            with open(path, 'wb') as f:
                f.write(self.data)
        else:
            img = self.to_pil()
            img.save(path, format=format)


class Font:
    """CSS-style font specification."""

    def __init__(
        self,
        family: Union[str, Path],
        size: float = 16.0,
        weight: int = 400,
        style: str = "normal",
        variations: Optional[Dict[str, float]] = None,
        features: Optional[Dict[str, bool]] = None,
    ):
        """Create a new font specification.

        Args:
            family: Font family name or path to font file
            size: Font size in pixels
            weight: Font weight (100-900, 400=normal, 700=bold)
            style: Font style ("normal", "italic", or "oblique")
            variations: Variable font axes (e.g., {"wght": 700, "wdth": 100})
            features: OpenType features (e.g., {"kern": True, "liga": False})

        Examples:
            >>> font = Font("Arial", 24)
            >>> font = Font("/path/to/font.ttf", 16, weight=700)
            >>> font = Font("Inter", 18, variations={"wght": 600})
        """
        if _Font is None:
            raise ImportError("o4e native module not available")

        self._font = _Font(str(family), size, weight, style)
        self.variations = variations or {}
        self.features = features or {}

    @property
    def family(self) -> str:
        """Font family name."""
        return self._font.family

    @property
    def size(self) -> float:
        """Font size in pixels."""
        return self._font.size

    @property
    def weight(self) -> int:
        """Font weight (100-900)."""
        return self._font.weight

    @property
    def style(self) -> str:
        """Font style (normal, italic, oblique)."""
        return self._font.style

    def with_size(self, size: float) -> 'Font':
        """Create a copy with different size."""
        return Font(self.family, size, self.weight, self.style,
                   self.variations.copy(), self.features.copy())

    def with_weight(self, weight: int) -> 'Font':
        """Create a copy with different weight."""
        return Font(self.family, self.size, weight, self.style,
                   self.variations.copy(), self.features.copy())

    def __repr__(self) -> str:
        return f"Font(family={self.family!r}, size={self.size}, weight={self.weight}, style={self.style!r})"


class TextRenderer:
    """High-performance multi-backend text renderer.

    Examples:
        >>> renderer = TextRenderer()
        >>> image = renderer.render("Hello", Font("Arial", 24))
        >>> renderer.render_to_file("Text", Font("Times", 18), "output.png")
    """

    def __init__(
        self,
        backend: Optional[str] = None,
        cache_size: int = 512,
        parallel: bool = True,
        timeout: Optional[float] = None,
    ):
        """Create a new text renderer.

        Args:
            backend: Backend to use ("coretext", "directwrite", "harfbuzz", "pure", or None for auto)
            cache_size: Number of shaped text results to cache
            parallel: Enable parallel processing for batch operations
            timeout: Maximum time in seconds for rendering operations

        Raises:
            ImportError: If native module is not available
            ValueError: If specified backend is not available
        """
        if _TextRenderer is None:
            raise ImportError("o4e native module not available")

        self._renderer = _TextRenderer(backend)
        self.cache_size = cache_size
        self.parallel = parallel
        self.timeout = timeout
        self._backend = backend or self._detect_backend()

    def _detect_backend(self) -> str:
        """Detect the active backend."""
        import platform
        system = platform.system()
        if system == "Darwin":
            return "coretext"
        elif system == "Windows":
            return "directwrite"
        else:
            return "harfbuzz"

    @property
    def backend(self) -> str:
        """Get the active backend name."""
        return self._backend

    def render(
        self,
        text: str,
        font: Union[Font, str],
        format: Union[str, RenderFormat] = RenderFormat.RAW,
        color: str = "#000000",
        background: str = "transparent",
        padding: int = 0,
        direction: Union[str, Direction] = Direction.AUTO,
        **options: Any
    ) -> Union[bytes, str, Bitmap]:
        """Render text to specified format.

        Args:
            text: Text to render
            font: Font specification or font family name
            format: Output format (RenderFormat.RAW/PNG/SVG or "raw"/"png"/"svg")
            color: Text color (hex, e.g., "#FF0000" for red)
            background: Background color (hex or "transparent")
            padding: Padding around text in pixels
            direction: Text direction for bidirectional text
            **options: Additional rendering options

        Returns:
            Rendered output:
            - Bitmap object for RAW format
            - bytes for PNG format
            - str (XML) for SVG format

        Examples:
            >>> renderer = TextRenderer()
            >>> bitmap = renderer.render("Hello", Font("Arial", 24))
            >>> png_data = renderer.render("World", "Times", format="png")
            >>> svg_xml = renderer.render("SVG", Font("Courier", 16), format=RenderFormat.SVG)
        """
        if isinstance(font, str):
            font = Font(font)

        if isinstance(format, RenderFormat):
            format = format.value
        if isinstance(direction, Direction):
            direction = direction.value

        # Prepare options
        render_options = {
            "color": color,
            "background": background,
            "padding": padding,
            "direction": direction,
            **options
        }

        # Call native render method
        result = self._renderer.render(
            text,
            font._font,
            format,
            render_options=render_options
        )

        # Convert raw data to Bitmap object
        if format == "raw" and isinstance(result, tuple):
            data, width, height = result
            return Bitmap(data, width, height)

        return result

    def shape(
        self,
        text: str,
        font: Union[Font, str],
        direction: Union[str, Direction] = Direction.AUTO,
        language: Optional[str] = None,
        script: Optional[str] = None,
        **options: Any
    ) -> 'ShapingResult':
        """Get shaping information without rendering.

        Args:
            text: Text to shape
            font: Font specification
            direction: Text direction
            language: Language code (e.g., "en", "ar", "hi")
            script: Script code (e.g., "Latn", "Arab", "Deva")
            **options: Additional shaping options

        Returns:
            ShapingResult with glyph information

        Examples:
            >>> result = renderer.shape("Hello", Font("Arial", 24))
            >>> for glyph in result.glyphs:
            ...     print(f"Glyph {glyph.id} at position {glyph.x}, {glyph.y}")
        """
        if isinstance(font, str):
            font = Font(font)

        if isinstance(direction, Direction):
            direction = direction.value

        shape_options = {
            "direction": direction,
            "language": language,
            "script": script,
            **options
        }

        return self._renderer.shape(
            text,
            font._font,
            shape_options=shape_options
        )

    def render_batch(
        self,
        items: List[Dict[str, Any]],
        format: Union[str, RenderFormat] = RenderFormat.PNG,
        max_workers: Optional[int] = None,
        progress_callback: Optional[callable] = None,
    ) -> List[Union[bytes, str, Bitmap]]:
        """Efficiently render multiple texts in parallel.

        Args:
            items: List of render specifications, each containing:
                   - "text": str (required)
                   - "font": Font or str (required)
                   - Additional options from render() method
            format: Output format for all items
            max_workers: Maximum number of parallel workers (None = CPU count)
            progress_callback: Function called with (completed, total) counts

        Returns:
            List of rendered outputs in same order as inputs

        Examples:
            >>> items = [
            ...     {"text": "Hello", "font": Font("Arial", 24)},
            ...     {"text": "World", "font": "Times", "color": "#FF0000"},
            ... ]
            >>> results = renderer.render_batch(items, format="png")
        """
        if not self.parallel:
            # Sequential processing
            results = []
            for i, item in enumerate(items):
                result = self.render(
                    item["text"],
                    item.get("font", "Arial"),
                    format=format,
                    **{k: v for k, v in item.items() if k not in ("text", "font")}
                )
                results.append(result)
                if progress_callback:
                    progress_callback(i + 1, len(items))
            return results

        # Parallel processing using native batch renderer
        native_format = format.value if isinstance(format, RenderFormat) else format

        native_items = []
        for item in items:
            entry = dict(item)
            font_value = entry.get("font", "Arial")
            if isinstance(font_value, Font):
                entry["font"] = font_value._font
            else:
                entry["font"] = Font(font_value)._font
            native_items.append(entry)

        return self._renderer.render_batch(native_items, native_format, max_workers)

    def render_to_file(
        self,
        text: str,
        font: Union[Font, str],
        path: Union[str, Path],
        format: Optional[Union[str, RenderFormat]] = None,
        **options: Any
    ):
        """Render text directly to a file.

        Args:
            text: Text to render
            font: Font specification
            path: Output file path
            format: Output format (auto-detected from extension if None)
            **options: Additional rendering options

        Examples:
            >>> renderer.render_to_file("Hello", Font("Arial", 24), "hello.png")
            >>> renderer.render_to_file("SVG Text", "Times", "text.svg")
        """
        path = Path(path)

        # Auto-detect format from extension
        if format is None:
            ext = path.suffix.lower()
            if ext == ".png":
                format = RenderFormat.PNG
            elif ext == ".svg":
                format = RenderFormat.SVG
            else:
                format = RenderFormat.RAW

        result = self.render(text, font, format=format, **options)

        if isinstance(result, Bitmap):
            result.save(path)
        elif isinstance(result, bytes):
            with open(path, 'wb') as f:
                f.write(result)
        elif isinstance(result, str):
            with open(path, 'w', encoding='utf-8') as f:
                f.write(result)

    def render_to_numpy(
        self,
        text: str,
        font: Union[Font, str],
        **options: Any
    ) -> 'np.ndarray':
        """Render text to a NumPy array.

        Args:
            text: Text to render
            font: Font specification
            **options: Rendering options

        Returns:
            NumPy array of shape (height, width, 4) with RGBA values

        Examples:
            >>> arr = renderer.render_to_numpy("Hello", Font("Arial", 24))
            >>> print(arr.shape)  # (height, width, 4)
        """
        bitmap = self.render(text, font, format=RenderFormat.RAW, **options)
        return bitmap.to_numpy()

    def render_to_pil(
        self,
        text: str,
        font: Union[Font, str],
        **options: Any
    ) -> 'PILImage.Image':
        """Render text to a PIL/Pillow Image.

        Args:
            text: Text to render
            font: Font specification
            **options: Rendering options

        Returns:
            PIL Image object

        Examples:
            >>> img = renderer.render_to_pil("Hello", Font("Arial", 24))
            >>> img.show()
        """
        bitmap = self.render(text, font, format=RenderFormat.RAW, **options)
        return bitmap.to_pil()

    def clear_cache(self):
        """Clear the internal cache."""
        self._renderer.clear_cache()

    def __repr__(self) -> str:
        return f"TextRenderer(backend={self.backend!r}, cache_size={self.cache_size}, parallel={self.parallel})"


# Convenience functions
def render(
    text: str,
    font: Union[str, Font] = "Arial",
    size: float = 16.0,
    format: Union[str, RenderFormat] = RenderFormat.RAW,
    **kwargs
) -> Union[bytes, str, Bitmap]:
    """Quick render function for simple use cases.

    Args:
        text: Text to render
        font: Font family name or Font object
        size: Font size (ignored if font is Font object)
        format: Output format
        **kwargs: Additional rendering options

    Returns:
        Rendered output

    Examples:
        >>> bitmap = render("Hello World", "Arial", 24)
        >>> png_data = render("Hello", Font("Times", 18), format="png")
    """
    if isinstance(font, str):
        font = Font(font, size)

    renderer = TextRenderer()
    return renderer.render(text, font, format=format, **kwargs)


def render_to_file(
    text: str,
    path: Union[str, Path],
    font: Union[str, Font] = "Arial",
    size: float = 16.0,
    **kwargs
):
    """Quick render to file for simple use cases.

    Args:
        text: Text to render
        path: Output file path
        font: Font family name or Font object
        size: Font size (ignored if font is Font object)
        **kwargs: Additional options

    Examples:
        >>> render_to_file("Hello", "hello.png", "Arial", 24)
        >>> render_to_file("World", "world.svg", Font("Times", 18))
    """
    if isinstance(font, str):
        font = Font(font, size)

    renderer = TextRenderer()
    renderer.render_to_file(text, font, path, **kwargs)


def shape_text(
    text: str,
    font: Union[str, Font] = "Arial",
    size: float = 16.0,
    **kwargs
) -> 'ShapingResult':
    """Quick shape function for getting glyph information.

    Args:
        text: Text to shape
        font: Font family name or Font object
        size: Font size (ignored if font is Font object)
        **kwargs: Additional shaping options

    Returns:
        ShapingResult with glyph information

    Examples:
        >>> result = shape_text("Hello", "Arial", 24)
        >>> print(f"Text width: {result.advance}")
    """
    if isinstance(font, str):
        font = Font(font, size)

    renderer = TextRenderer()
    return renderer.shape(text, font, **kwargs)


def list_backends() -> List[str]:
    """List available rendering backends.

    Returns:
        List of backend names

    Examples:
        >>> backends = list_backends()
        >>> print(backends)  # ['coretext', 'harfbuzz', ...]
    """
    backends = ["harfbuzz", "pure"]  # Always available

    import platform
    system = platform.system()

    if system == "Darwin":
        backends.insert(0, "coretext")
    elif system == "Windows":
        backends.insert(0, "directwrite")

    # Check for optional Skia backend
    try:
        import o4e_skia
        backends.append("skia")
    except ImportError:
        pass

    return backends


def get_default_backend() -> str:
    """Get the default backend for the current platform.

    Returns:
        Backend name

    Examples:
        >>> backend = get_default_backend()
        >>> print(backend)  # 'coretext' on macOS, 'directwrite' on Windows, etc.
    """
    import platform
    system = platform.system()

    if system == "Darwin":
        return "coretext"
    elif system == "Windows":
        return "directwrite"
    else:
        return "harfbuzz"


# Batch processing helper
class BatchProcessor:
    """Helper for processing large batches of text with progress tracking.

    Examples:
        >>> processor = BatchProcessor()
        >>> texts = ["Hello", "World", "Test"]
        >>> fonts = [Font("Arial", 24), Font("Times", 18), Font("Courier", 16)]
        >>> results = processor.process(texts, fonts)
    """

    def __init__(self, renderer: Optional[TextRenderer] = None):
        """Initialize batch processor.

        Args:
            renderer: TextRenderer instance (creates new if None)
        """
        self.renderer = renderer or TextRenderer(parallel=True)

    def process(
        self,
        texts: List[str],
        fonts: Union[Font, List[Font]],
        format: Union[str, RenderFormat] = RenderFormat.PNG,
        progress: bool = False,
        **options: Any
    ) -> List[Union[bytes, str, Bitmap]]:
        """Process a batch of texts.

        Args:
            texts: List of texts to render
            fonts: Single font for all texts or list of fonts
            format: Output format
            progress: Show progress bar (requires tqdm)
            **options: Additional rendering options

        Returns:
            List of rendered outputs
        """
        # Prepare items
        if isinstance(fonts, Font):
            fonts = [fonts] * len(texts)
        elif len(fonts) != len(texts):
            raise ValueError(f"Number of fonts ({len(fonts)}) must match texts ({len(texts)})")

        items = []
        for text, font in zip(texts, fonts):
            item = {"text": text, "font": font}
            item.update(options)
            items.append(item)

        # Setup progress tracking
        progress_callback = None
        if progress:
            try:
                from tqdm import tqdm
                pbar = tqdm(total=len(items), desc="Rendering")

                def progress_callback(completed, total):
                    pbar.update(1)
            except ImportError:
                warnings.warn("tqdm not installed, progress bar disabled")

        # Process batch
        results = self.renderer.render_batch(items, format=format, progress_callback=progress_callback)

        if progress and 'pbar' in locals():
            pbar.close()

        return results


# Version check
def check_version() -> Dict[str, Any]:
    """Check o4e version and available features.

    Returns:
        Dictionary with version info and available features

    Examples:
        >>> info = check_version()
        >>> print(info["version"])
        >>> print(info["backends"])
    """
    return {
        "version": __version__,
        "backends": list_backends(),
        "default_backend": get_default_backend(),
        "has_pil": HAS_PIL,
        "has_numpy": HAS_NUMPY,
        "features": {
            "batch_processing": True,
            "svg_output": True,
            "png_output": True,
            "unicode_support": True,
            "variable_fonts": True,
            "opentype_features": True,
        }
    }


# Module initialization message (optional, can be disabled)
if __name__ != "__main__":
    import os
    if os.environ.get("O4E_VERBOSE"):
        info = check_version()
        print(f"o4e {info['version']} loaded with backends: {', '.join(info['backends'])}")
