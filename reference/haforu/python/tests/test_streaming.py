# this_file: python/tests/test_streaming.py

"""Tests for haforu streaming session Python bindings."""

import json
import pytest


def test_streaming_session_import():
    """Test that StreamingSession class can be imported."""
    try:
        import haforu
        assert hasattr(haforu, "StreamingSession")
    except ImportError:
        pytest.skip("haforu Python bindings not installed")


def test_streaming_session_creation():
    """Test that StreamingSession can be created."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
    assert session is not None


def test_streaming_session_custom_cache_size():
    """Test that StreamingSession accepts custom cache size."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession(cache_size=1024)
    assert session is not None


def test_streaming_session_cache_stats_and_resize():
    """StreamingSession exposes cache stats + resize knob."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession(cache_size=128)
    stats = session.cache_stats()
    assert "capacity" in stats and "entries" in stats
    assert stats["capacity"] == 128

    session.set_cache_size(64)
    resized = session.cache_stats()
    assert resized["capacity"] == 64


def test_streaming_session_close():
    """Test that StreamingSession can be closed."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
    session.close()
    # Follow-up renders should raise RuntimeError once closed
    job = json.dumps(
        {
            "id": "after-close",
            "font": {"path": "/nonexistent/font.ttf", "size": 1000, "variations": {}},
            "text": {"content": "a"},
            "rendering": {"format": "pgm", "encoding": "base64", "width": 32, "height": 32},
        }
    )
    with pytest.raises(RuntimeError):
        session.render(job)


def test_streaming_session_context_manager():
    """Test that StreamingSession works as context manager."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    with haforu.StreamingSession() as session:
        assert session is not None
    # Should not raise error on exit


def test_streaming_session_render_method_exists():
    """Test that StreamingSession has render method."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
    assert hasattr(session, "render")
    assert hasattr(session, "warm_up")


def test_streaming_session_render_invalid_json():
    """Test that render raises error for invalid JSON."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
    with pytest.raises(ValueError, match="Invalid JSON"):
        session.render("not valid json")


def test_streaming_session_warm_up_ping():
    """warm_up should succeed without a font path."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
    assert session.warm_up() is True


def test_streaming_session_render_single_job():
    """Test that render processes a single job and returns JSONL."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()
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
    job_json = json.dumps(job)
    result_json = session.render(job_json)

    # Parse result
    result = json.loads(result_json)
    assert "id" in result
    assert result["id"] == "test1"
    assert "status" in result
    # Will be "error" because font doesn't exist
    assert result["status"] in ["success", "error"]
    assert "timing" in result


def test_streaming_session_multiple_renders():
    """Test that session can handle multiple sequential renders."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()

    # Render same job 10 times
    for i in range(10):
        job = {
            "id": f"test{i}",
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
        job_json = json.dumps(job)
        result_json = session.render(job_json)
        result = json.loads(result_json)
        assert result["id"] == f"test{i}"


def test_streaming_session_result_format():
    """Test that streaming session results match expected format."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    with haforu.StreamingSession() as session:
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

        # Check required fields
        assert "id" in result
        assert "status" in result
        assert "timing" in result

        # Check timing structure
        timing = result["timing"]
        assert "total_ms" in timing
        # Other timing fields may vary (render_ms, shape_ms, etc.)


def test_streaming_session_error_handling():
    """Test that streaming session handles errors gracefully."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    session = haforu.StreamingSession()


def test_haforu_module_is_available_probe():
    """Module-level availability probe should be fast and boolean."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    available = haforu.is_available()
    assert isinstance(available, bool)

    # Test with missing required field
    job = {
        "id": "test1",
        "font": {"path": "/nonexistent/font.ttf", "size": 1000},
        # Missing text field
        "rendering": {
            "format": "pgm",
            "encoding": "base64",
            "width": 100,
            "height": 100,
        },
    }

    # Should either raise ValueError or return error result
    try:
        result_json = session.render(json.dumps(job))
        result = json.loads(result_json)
        assert result["status"] == "error"
    except ValueError:
        # Also acceptable
        pass
