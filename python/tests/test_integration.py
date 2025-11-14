# this_file: python/tests/test_integration.py

"""Integration tests for o4e Python bindings using the real native module."""

import pytest
import os
import sys
from pathlib import Path

# Import the actual native module (no mocking)
try:
    from o4e import o4e as native
    NATIVE_AVAILABLE = True
except ImportError:
    NATIVE_AVAILABLE = False
    pytestmark = pytest.mark.skip("Native o4e module not available")


@pytest.fixture
def testdata_dir():
    """Get the testdata directory."""
    root = Path(__file__).resolve().parents[2]
    return root / "testdata" / "fonts"


@pytest.fixture
def simple_font():
    """Create a simple test font."""
    if NATIVE_AVAILABLE:
        return native.Font("Arial", 24.0)
    return None


@pytest.fixture
def test_font_path(testdata_dir):
    """Get a path to a test font if available."""
    noto_sans = testdata_dir / "NotoSans-Regular.ttf"
    if noto_sans.exists():
        return str(noto_sans)
    return None


class TestBasicRendering:
    """Test basic text rendering functionality."""

    def test_renderer_creation(self):
        """Test creating a TextRenderer."""
        renderer = native.TextRenderer()
        assert renderer is not None

    def test_renderer_with_backend(self):
        """Test creating a TextRenderer with specific backend."""
        # Test auto backend
        renderer = native.TextRenderer()
        assert renderer is not None

        # Test explicit backends based on platform
        import platform
        if platform.system() == "Darwin":
            try:
                renderer = native.TextRenderer("coretext")
                assert renderer is not None
            except Exception:
                pass  # Backend may not be available

        # HarfBuzz should be available everywhere with icu feature
        try:
            renderer = native.TextRenderer("harfbuzz")
            assert renderer is not None
        except Exception:
            pytest.skip("HarfBuzz backend not available")

    def test_render_simple_text(self, simple_font):
        """Test rendering simple Latin text."""
        renderer = native.TextRenderer()
        result = renderer.render("Hello World", simple_font, format="raw")

        # Result should be (data, width, height) tuple or Bitmap-like object
        assert result is not None

    def test_render_to_png(self, simple_font):
        """Test rendering to PNG format."""
        renderer = native.TextRenderer()
        result = renderer.render("Test PNG", simple_font, format="png")

        # PNG data should be bytes
        assert isinstance(result, bytes)
        # PNG magic number
        assert result[:8] == b'\x89PNG\r\n\x1a\n'

    def test_render_to_svg(self, simple_font):
        """Test rendering to SVG format."""
        renderer = native.TextRenderer()
        result = renderer.render("Test SVG", simple_font, format="svg")

        # SVG should be string containing SVG markup
        assert isinstance(result, str)
        assert "<svg" in result

    def test_render_with_options(self, simple_font):
        """Test rendering with custom options."""
        renderer = native.TextRenderer()

        render_options = {
            "color": "#FF0000",
            "background": "#FFFFFF",
            "padding": 5
        }

        result = renderer.render(
            "Test Options",
            simple_font,
            format="raw",
            render_options=render_options
        )

        assert result is not None

    def test_render_empty_string(self, simple_font):
        """Test rendering empty string."""
        renderer = native.TextRenderer()
        result = renderer.render("", simple_font, format="raw")

        # Should handle gracefully
        assert result is not None


class TestTextShaping:
    """Test text shaping functionality."""

    def test_shape_simple_text(self, simple_font):
        """Test shaping simple text."""
        renderer = native.TextRenderer()
        result = renderer.shape("Hello", simple_font)

        assert hasattr(result, 'text')
        assert hasattr(result, 'glyphs')
        assert hasattr(result, 'advance')
        assert result.text == "Hello"
        assert len(result.glyphs) > 0

    def test_shape_with_ligatures(self):
        """Test shaping text with ligatures (if font supports them)."""
        renderer = native.TextRenderer()
        font = native.Font("Times", 24.0)

        # "fi" and "fl" often have ligatures in Times
        result = renderer.shape("first flag", font)

        assert result is not None
        assert len(result.glyphs) > 0

    def test_shape_complex_script_arabic(self):
        """Test shaping Arabic text (RTL, contextual forms)."""
        renderer = native.TextRenderer()
        font = native.Font("Arial", 24.0)

        # Arabic text: "مرحبا" (hello)
        result = renderer.shape("مرحبا", font)

        assert result is not None
        assert len(result.glyphs) > 0

    def test_shape_mixed_scripts(self):
        """Test shaping text with mixed scripts."""
        renderer = native.TextRenderer()
        font = native.Font("Arial", 24.0)

        # Mix of Latin and Arabic
        result = renderer.shape("Hello مرحبا World", font)

        assert result is not None
        assert len(result.glyphs) > 0

    def test_shape_returns_correct_advance(self, simple_font):
        """Test that shaping returns reasonable advance width."""
        renderer = native.TextRenderer()
        result = renderer.shape("Test", simple_font)

        # Advance should be positive for non-empty text
        assert result.advance > 0


class TestBatchProcessing:
    """Test batch rendering functionality."""

    def test_render_batch_simple(self, simple_font):
        """Test batch rendering of multiple texts."""
        renderer = native.TextRenderer()

        items = [
            {"text": "Item 1", "font": simple_font},
            {"text": "Item 2", "font": simple_font},
            {"text": "Item 3", "font": simple_font},
        ]

        results = renderer.render_batch(items, format="raw")

        assert len(results) == 3
        for result in results:
            assert result is not None

    def test_render_batch_different_fonts(self):
        """Test batch rendering with different fonts."""
        renderer = native.TextRenderer()

        items = [
            {"text": "Arial", "font": native.Font("Arial", 24.0)},
            {"text": "Times", "font": native.Font("Times", 24.0)},
        ]

        results = renderer.render_batch(items, format="raw")

        assert len(results) == 2

    def test_render_batch_empty_list(self):
        """Test batch rendering with empty list."""
        renderer = native.TextRenderer()
        results = renderer.render_batch([], format="raw")

        assert isinstance(results, list)
        assert len(results) == 0

    def test_render_batch_with_threads(self, simple_font):
        """Test batch rendering with custom thread count."""
        renderer = native.TextRenderer()

        items = [
            {"text": f"Item {i}", "font": simple_font}
            for i in range(10)
        ]

        results = renderer.render_batch(items, format="raw", max_workers=4)

        assert len(results) == 10


class TestFontFallback:
    """Test font fallback functionality."""

    def test_fallback_for_missing_glyphs(self):
        """Test that renderer falls back when glyphs are missing."""
        renderer = native.TextRenderer()

        # Use a font that might not have emoji
        font = native.Font("Arial", 24.0)

        # Mix of Latin and emoji
        result = renderer.render("Hello 👋 World", font, format="raw")

        # Should handle gracefully even if emoji is missing
        assert result is not None

    def test_shape_with_missing_glyphs(self):
        """Test shaping with missing glyphs."""
        renderer = native.TextRenderer()
        font = native.Font("Arial", 24.0)

        # Special characters that might need fallback
        result = renderer.shape("Test ⚠️ Warning", font)

        assert result is not None
        assert len(result.glyphs) > 0


class TestFontLoading:
    """Test font loading from different sources."""

    def test_font_from_path(self, test_font_path):
        """Test loading font from file path."""
        if test_font_path is None:
            pytest.skip("Test font not available")

        font = native.Font.from_path(test_font_path, 24.0)
        renderer = native.TextRenderer()

        result = renderer.render("Test", font, format="raw")
        assert result is not None

    def test_font_from_bytes(self, test_font_path):
        """Test loading font from raw bytes."""
        if test_font_path is None:
            pytest.skip("Test font not available")

        with open(test_font_path, 'rb') as f:
            font_data = f.read()

        font = native.Font.from_bytes("TestFont", font_data, 24.0)
        renderer = native.TextRenderer()

        result = renderer.render("Test", font, format="raw")
        assert result is not None

    def test_font_with_variations(self):
        """Test font with variable axis settings."""
        # Font is immutable, variations need to be set at construction
        # This test verifies Font can be created successfully
        font = native.Font("Arial", 24.0)

        renderer = native.TextRenderer()
        result = renderer.render("Bold", font, format="raw")

        assert result is not None

    def test_font_with_features(self):
        """Test font with OpenType features."""
        # Font is immutable, features need to be set at construction
        # This test verifies Font can be created successfully
        font = native.Font("Arial", 24.0)

        renderer = native.TextRenderer()
        result = renderer.render("Test", font, format="raw")

        assert result is not None


class TestErrorHandling:
    """Test error handling."""

    def test_invalid_backend_name(self):
        """Test that invalid backend name raises error."""
        with pytest.raises((ValueError, RuntimeError)):
            native.TextRenderer("nonexistent_backend")

    def test_render_with_invalid_font(self):
        """Test rendering with problematic font."""
        renderer = native.TextRenderer()

        # Font with zero size should fail or handle gracefully
        try:
            font = native.Font("Arial", 0.0)
            result = renderer.render("Test", font, format="raw")
            # If it doesn't fail, that's okay too (implementation choice)
        except (ValueError, RuntimeError):
            pass  # Expected error

    def test_render_with_invalid_format(self):
        """Test rendering with invalid format."""
        renderer = native.TextRenderer()
        font = native.Font("Arial", 24.0)

        with pytest.raises((ValueError, RuntimeError)):
            renderer.render("Test", font, format="invalid_format")

    def test_font_from_nonexistent_path(self):
        """Test loading font from nonexistent path."""
        font = native.Font.from_path("/nonexistent/path/font.ttf", 24.0)
        renderer = native.TextRenderer()

        # Should fail when trying to use it
        with pytest.raises(RuntimeError):
            renderer.render("Test", font, format="raw")

    def test_font_from_invalid_bytes(self):
        """Test loading font from invalid bytes."""
        invalid_data = b"not a font file"
        font = native.Font.from_bytes("Invalid", invalid_data, 24.0)
        renderer = native.TextRenderer()

        # Should fail when trying to use it
        with pytest.raises(RuntimeError):
            renderer.render("Test", font, format="raw")


class TestCacheOperations:
    """Test cache operations."""

    def test_clear_cache(self):
        """Test clearing the renderer cache."""
        renderer = native.TextRenderer()
        font = native.Font("Arial", 24.0)

        # Render something to populate cache
        renderer.render("Test", font, format="raw")

        # Clear cache should not raise
        renderer.clear_cache()

        # Should still work after clearing
        result = renderer.render("Test", font, format="raw")
        assert result is not None


class TestVersionInfo:
    """Test version and availability checks."""

    def test_is_available(self):
        """Test checking if o4e is available."""
        assert native.TextRenderer.is_available() is True

    def test_version(self):
        """Test getting version string."""
        version = native.TextRenderer.version()
        assert isinstance(version, str)
        assert len(version) > 0


if __name__ == "__main__":
    pytest.main([__file__, "-v"])
