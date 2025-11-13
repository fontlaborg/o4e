# this_file: python/tests/test_batch.py

"""Tests for haforu batch mode Python bindings."""

import json
import pytest


def test_import_haforu():
    """Test that haforu module can be imported."""
    try:
        import haforu
        assert haforu.__version__ == "2.0.0"
    except ImportError:
        pytest.skip("haforu Python bindings not installed")


def test_process_jobs_function_exists():
    """Test that process_jobs function is exported."""
    try:
        import haforu
        assert hasattr(haforu, "process_jobs")
    except ImportError:
        pytest.skip("haforu Python bindings not installed")


def test_process_jobs_empty_list():
    """Test that process_jobs raises error for empty job list."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    spec = {"version": "1.0", "jobs": []}
    spec_json = json.dumps(spec)

    with pytest.raises(ValueError, match="empty"):
        list(haforu.process_jobs(spec_json))


def test_process_jobs_invalid_json():
    """Test that process_jobs raises error for invalid JSON."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    with pytest.raises(ValueError, match="Invalid JSON"):
        list(haforu.process_jobs("not valid json"))


def test_process_jobs_invalid_version():
    """Test that process_jobs raises error for unsupported version."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    spec = {
        "version": "2.0",
        "jobs": [
            {
                "id": "test1",
                "font": {"path": "/path/to/font.ttf", "size": 1000, "variations": {}},
                "text": {"content": "a"},
                "rendering": {
                    "format": "pgm",
                    "encoding": "base64",
                    "width": 100,
                    "height": 100,
                },
            }
        ],
    }
    spec_json = json.dumps(spec)

    with pytest.raises(ValueError, match="Unsupported version"):
        list(haforu.process_jobs(spec_json))


def test_process_jobs_basic_structure():
    """Test that process_jobs returns an iterator."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    # Note: This test will fail if font doesn't exist, but tests the structure
    spec = {
        "version": "1.0",
        "jobs": [
            {
                "id": "test1",
                "font": {
                    "path": "/nonexistent/font.ttf",
                    "size": 1000,
                    "variations": {},
                },
                "text": {"content": "a", "script": "Latn"},
                "rendering": {
                    "format": "pgm",
                    "encoding": "base64",
                    "width": 100,
                    "height": 100,
                },
            }
        ],
    }
    spec_json = json.dumps(spec)

    results = haforu.process_jobs(spec_json)
    assert hasattr(results, "__iter__")
    assert hasattr(results, "__next__")


def test_process_jobs_result_format():
    """Test that process_jobs returns valid JSON results."""
    try:
        import haforu
    except ImportError:
        pytest.skip("haforu Python bindings not installed")

    # This will return an error result for nonexistent font
    spec = {
        "version": "1.0",
        "jobs": [
            {
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
        ],
    }
    spec_json = json.dumps(spec)

    results = list(haforu.process_jobs(spec_json))
    assert len(results) == 1

    # Parse result
    result = json.loads(results[0])
    assert "id" in result
    assert result["id"] == "test1"
    assert "status" in result
    # Will be "error" because font doesn't exist
    assert result["status"] in ["success", "error"]
    assert "timing" in result
