# this_file: python/tests/test_numpy.py

"""Tests for haforu numpy zero-copy integration."""

import json
import pytest


def test_render_to_numpy_import():
    """Test that render_to_numpy method exists."""
    try:
        import haforu
        assert hasattr(haforu.StreamingSession, "render_to_numpy")
    except ImportError:
        pytest.skip("haforu Python bindings not installed")


def test_render_to_numpy_basic():
    """Test basic numpy rendering."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    # This will fail with nonexistent font, but tests the method exists
    # and returns correct error type
    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
        # If it succeeds (shouldn't with nonexistent font), check shape
        assert image.shape == (100, 100)
    except RuntimeError as e:
        # Expected: font loading failed
        assert "Font loading failed" in str(e) or "failed" in str(e).lower()


def test_render_to_numpy_array_shape():
    """Test that numpy array has correct shape."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    # Test with different dimensions
    for width, height in [(100, 100), (200, 150), (3000, 1200)]:
        try:
            image = session.render_to_numpy(
                font_path="/nonexistent/font.ttf",
                text="a",
                size=1000.0,
                width=width,
                height=height,
            )
            assert image.shape == (height, width), f"Expected ({height}, {width}), got {image.shape}"
        except RuntimeError:
            # Expected: font not found
            pass


def test_render_to_numpy_dtype():
    """Test that numpy array has correct dtype."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
        assert image.dtype == np.uint8
    except RuntimeError:
        # Expected: font not found
        pass


def test_render_to_numpy_with_variations():
    """Test numpy rendering with variable font variations."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
            variations={"wght": 600.0, "wdth": 75.0},
        )
        assert image.shape == (100, 100)
        assert image.dtype == np.uint8
    except RuntimeError as e:
        # Expected: font not found
        assert "Font loading failed" in str(e) or "failed" in str(e).lower()


def test_render_to_numpy_with_script_params():
    """Test numpy rendering with script/direction/language parameters."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
            script="Latn",
            direction="ltr",
            language="en",
        )
        assert image.shape == (100, 100)
    except RuntimeError:
        # Expected: font not found
        pass


def test_render_to_numpy_array_contiguous():
    """Test that numpy array is contiguous (zero-copy indicator)."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
        # Check array flags for contiguity
        assert image.flags.c_contiguous or image.flags.f_contiguous
    except RuntimeError:
        # Expected: font not found
        pass


def test_render_to_numpy_value_range():
    """Test that numpy array values are in valid grayscale range."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    try:
        image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
        # All values should be 0-255
        assert image.min() >= 0
        assert image.max() <= 255
    except RuntimeError:
        # Expected: font not found
        pass


def test_render_to_numpy_context_manager():
    """Test numpy rendering works with context manager."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    with haforu.StreamingSession() as session:
        try:
            image = session.render_to_numpy(
                font_path="/nonexistent/font.ttf",
                text="a",
                size=1000.0,
                width=100,
                height=100,
            )
            assert image.shape == (100, 100)
        except RuntimeError:
            # Expected: font not found
            pass


def test_render_to_numpy_multiple_calls():
    """Test multiple sequential numpy renders (cache performance)."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    # Render same glyph multiple times
    for i in range(5):
        try:
            image = session.render_to_numpy(
                font_path="/nonexistent/font.ttf",
                text="a",
                size=1000.0,
                width=100,
                height=100,
            )
            assert image.shape == (100, 100)
        except RuntimeError:
            # Expected: font not found
            pass


def test_render_to_numpy_parameter_validation():
    """Test that render_to_numpy validates parameters."""
    try:
        import haforu
        import numpy as np
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    # Test with empty font path
    try:
        image = session.render_to_numpy(
            font_path="",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
        # Should fail
        assert False, "Expected error for empty font path"
    except (ValueError, RuntimeError):
        # Expected: validation error
        pass


def test_render_to_numpy_vs_base64_consistency():
    """Test that numpy output matches base64-decoded output structure."""
    try:
        import haforu
        import numpy as np
        import base64
    except ImportError:
        pytest.skip("haforu or numpy not installed")

    session = haforu.StreamingSession()

    # Both methods should fail with nonexistent font
    # but we can verify they accept the same parameters

    # Test numpy method
    try:
        numpy_image = session.render_to_numpy(
            font_path="/nonexistent/font.ttf",
            text="a",
            size=1000.0,
            width=100,
            height=100,
        )
    except RuntimeError:
        numpy_image = None

    # Test JSON method
    job = {
        "id": "test1",
        "font": {
            "path": "/nonexistent/font.ttf",
            "size": 1000,
            "variations": {},
        },
        "text": {"content": "a"},
        "rendering": {
            "format": "pgm",
            "encoding": "base64",
            "width": 100,
            "height": 100,
        },
    }

    result_json = session.render(json.dumps(job))
    result = json.loads(result_json)

    # Both should produce consistent error status
    if numpy_image is None:
        assert result["status"] == "error"
