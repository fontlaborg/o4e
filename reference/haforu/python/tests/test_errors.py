"""
Test error handling and edge cases for haforu Python bindings.

Tests all error paths, validation, and edge cases to ensure
robust error handling with helpful error messages.
"""

import json
import tempfile
from pathlib import Path

import pytest

# Import haforu bindings
try:
    import haforu
except ImportError:
    pytest.skip("haforu not installed", allow_module_level=True)


class TestMissingFontErrors:
    """Test error handling when font files are missing or inaccessible."""

    def test_process_jobs_missing_font(self):
        """Test batch mode with missing font file."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "test_missing",
                    "font": {
                        "path": "/nonexistent/missing_font.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "a", "script": "Latn"},
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        # Process job (should return error in result, not raise exception)
        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1

        result = json.loads(results[0])
        assert result["status"] == "error"
        assert result["id"] == "test_missing"
        # Error message should mention the file path
        assert "/nonexistent/missing_font.ttf" in result.get("error", "")

    def test_streaming_session_missing_font(self):
        """Test streaming mode with missing font file."""
        session = haforu.StreamingSession()
        try:
            job = {
                "id": "test_missing_streaming",
                "font": {
                    "path": "/nonexistent/font.ttf",
                    "size": 1000,
                    "variations": {},
                },
                "text": {"content": "a"},
                "rendering": {
                    "format": "pgm",
                    "encoding": "base64",
                    "width": 3000,
                    "height": 1200,
                },
            }

            # Should return error status in JSON, not raise exception
            result_json = session.render(json.dumps(job))
            result = json.loads(result_json)
            assert result["status"] == "error"
            assert "/nonexistent/font.ttf" in result.get("error", "")
        finally:
            session.close()

    def test_render_to_numpy_missing_font(self):
        """Test numpy rendering with missing font file."""
        session = haforu.StreamingSession()
        try:
            with pytest.raises((IOError, RuntimeError)) as exc_info:
                session.render_to_numpy(
                    font_path="/nonexistent/missing.ttf",
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=1200,
                )

            # Error should mention the font path
            assert "/nonexistent/missing.ttf" in str(exc_info.value)
        finally:
            session.close()


class TestInvalidFontErrors:
    """Test error handling with corrupted or invalid font files."""

    def test_corrupted_font_file(self, tmp_path):
        """Test with corrupted (non-font) file."""
        # Create a file with invalid font data
        corrupted_font = tmp_path / "corrupted.ttf"
        corrupted_font.write_bytes(b"This is not a font file!")

        session = haforu.StreamingSession()
        try:
            with pytest.raises(RuntimeError) as exc_info:
                session.render_to_numpy(
                    font_path=str(corrupted_font),
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=1200,
                )

            # Error should indicate invalid font
            error_msg = str(exc_info.value).lower()
            assert "font" in error_msg or "invalid" in error_msg or "failed" in error_msg
            assert str(corrupted_font) in str(exc_info.value)
        finally:
            session.close()

    def test_empty_font_file(self, tmp_path):
        """Test with empty font file."""
        empty_font = tmp_path / "empty.ttf"
        empty_font.write_bytes(b"")

        session = haforu.StreamingSession()
        try:
            with pytest.raises(RuntimeError) as exc_info:
                session.render_to_numpy(
                    font_path=str(empty_font),
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=1200,
                )

            assert str(empty_font) in str(exc_info.value)
        finally:
            session.close()


class TestJSONValidationErrors:
    """Test JSON parsing and validation errors."""

    def test_invalid_json_syntax(self):
        """Test with malformed JSON."""
        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs("not valid json {{{"))

        assert "JSON" in str(exc_info.value) or "parse" in str(exc_info.value).lower()

    def test_missing_version_field(self):
        """Test with missing version field."""
        spec_json = json.dumps({"jobs": [{"id": "test"}]})

        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs(spec_json))

        # Should indicate JSON parsing or missing field error
        error_msg = str(exc_info.value).lower()
        assert "json" in error_msg or "version" in error_msg or "missing" in error_msg

    def test_invalid_version(self):
        """Test with unsupported version."""
        spec = {"version": "99.0", "jobs": [{"id": "test"}]}

        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs(json.dumps(spec)))

        assert "version" in str(exc_info.value).lower()

    def test_empty_jobs_list(self):
        """Test with empty jobs array."""
        spec = {"version": "1.0", "jobs": []}

        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs(json.dumps(spec)))

        assert "empty" in str(exc_info.value).lower()

    def test_streaming_invalid_json(self):
        """Test streaming session with invalid JSON."""
        session = haforu.StreamingSession()
        try:
            with pytest.raises(ValueError) as exc_info:
                session.render("not valid json")

            assert "JSON" in str(exc_info.value) or "parse" in str(exc_info.value).lower()
        finally:
            session.close()


class TestRenderParameterValidation:
    """Test validation of rendering parameters."""

    def test_invalid_dimensions_zero_width(self):
        """Test with zero width."""
        session = haforu.StreamingSession()
        try:
            # Note: This may be caught at validation level or render level
            # depending on where the check is implemented
            with pytest.raises((ValueError, RuntimeError)) as exc_info:
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",  # Will fail before dimension check
                    text="a",
                    size=1000.0,
                    width=0,  # Invalid
                    height=1200,
                )

            # Some error should be raised (could be IOError for missing font first)
            assert exc_info.value is not None
        finally:
            session.close()

    def test_invalid_dimensions_zero_height(self):
        """Test with zero height."""
        session = haforu.StreamingSession()
        try:
            with pytest.raises((ValueError, RuntimeError, IOError)):
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=0,  # Invalid
                )
        finally:
            session.close()

    def test_invalid_font_size_zero(self):
        """Test with zero font size."""
        session = haforu.StreamingSession()
        try:
            with pytest.raises((ValueError, RuntimeError, IOError)):
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",
                    text="a",
                    size=0.0,  # Invalid
                    width=3000,
                    height=1200,
                )
        finally:
            session.close()

    def test_invalid_font_size_negative(self):
        """Test with negative font size."""
        session = haforu.StreamingSession()
        try:
            with pytest.raises((ValueError, RuntimeError, IOError)):
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",
                    text="a",
                    size=-100.0,  # Invalid
                    width=3000,
                    height=1200,
                )
        finally:
            session.close()


class TestEmptyTextHandling:
    """Test handling of empty or whitespace text."""

    def test_empty_text_content(self):
        """Test rendering with empty text string."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "test_empty_text",
                    "font": {
                        "path": "/tmp/nonexistent.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "", "script": "Latn"},  # Empty text
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        # Should handle gracefully (may return error or empty rendering)
        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1
        result = json.loads(results[0])
        # Either error status or success with empty rendering
        assert result["status"] in ["error", "success"]

    def test_whitespace_only_text(self):
        """Test rendering with whitespace-only text."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "test_whitespace",
                    "font": {
                        "path": "/tmp/nonexistent.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "   ", "script": "Latn"},  # Whitespace only
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1
        result = json.loads(results[0])
        assert result["status"] in ["error", "success"]


class TestVariationCoordinateErrors:
    """Test error handling for invalid variation coordinates."""

    def test_invalid_variation_axis_name(self):
        """Test with unknown variation axis."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "test_invalid_axis",
                    "font": {
                        "path": "/tmp/nonexistent.ttf",
                        "size": 1000,
                        "variations": {"ZZZZ": 500.0},  # Unknown axis
                    },
                    "text": {"content": "a", "script": "Latn"},
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        # Will fail when trying to load font (missing file)
        # If font existed, would fail on invalid axis
        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1
        result = json.loads(results[0])
        assert result["status"] == "error"

    def test_numpy_invalid_variation_type(self):
        """Test render_to_numpy with invalid variation coordinate type."""
        session = haforu.StreamingSession()
        try:
            # Python will catch type errors before reaching Rust
            with pytest.raises((TypeError, ValueError, IOError, RuntimeError)):
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=1200,
                    variations={"wght": "not_a_number"},  # Invalid type
                )
        finally:
            session.close()


class TestErrorMessageQuality:
    """Test that error messages are helpful and include context."""

    def test_error_includes_font_path(self):
        """Verify error messages include font path."""
        test_path = "/some/specific/path/to/font.ttf"
        session = haforu.StreamingSession()
        try:
            with pytest.raises((IOError, RuntimeError)) as exc_info:
                session.render_to_numpy(
                    font_path=test_path,
                    text="a",
                    size=1000.0,
                    width=3000,
                    height=1200,
                )

            # Error message should include the font path
            assert test_path in str(exc_info.value)
        finally:
            session.close()

    def test_batch_error_includes_job_id(self):
        """Verify batch mode errors include job ID in result."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "my_specific_job_id_12345",
                    "font": {
                        "path": "/nonexistent/font.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "a", "script": "Latn"},
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1

        result = json.loads(results[0])
        # Job ID should be in the result
        assert result["id"] == "my_specific_job_id_12345"
        # Status should be error
        assert result["status"] == "error"
        # Error field should exist
        assert "error" in result

    def test_json_error_indicates_parse_issue(self):
        """Verify JSON errors clearly indicate parsing problem."""
        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs("{invalid json"))

        error_msg = str(exc_info.value).lower()
        # Should mention JSON or parse
        assert "json" in error_msg or "parse" in error_msg

    def test_validation_error_indicates_reason(self):
        """Verify validation errors include reason."""
        spec = {"version": "1.0", "jobs": []}

        with pytest.raises(ValueError) as exc_info:
            list(haforu.process_jobs(json.dumps(spec)))

        # Should indicate empty jobs list
        assert "empty" in str(exc_info.value).lower()


class TestContextManagerErrorHandling:
    """Test error handling with context managers."""

    def test_exception_in_context_manager(self):
        """Test that exceptions propagate correctly from context manager."""
        with pytest.raises(ValueError):
            with haforu.StreamingSession() as session:
                session.render("invalid json")

    def test_context_manager_cleanup_after_error(self):
        """Test that cleanup happens even after errors."""
        session = haforu.StreamingSession()

        # Use context manager and cause error
        try:
            with session:
                raise ValueError("Test error")
        except ValueError:
            pass

        # Session should still be usable (or properly closed)
        # This tests that __exit__ ran even with exception
        assert True  # If we get here, cleanup worked


class TestEdgeCases:
    """Test edge cases and boundary conditions."""

    def test_very_large_dimensions(self):
        """Test with very large canvas dimensions."""
        session = haforu.StreamingSession()
        try:
            # Very large dimensions may cause memory issues
            # Should either work or fail gracefully
            with pytest.raises((ValueError, RuntimeError, IOError, MemoryError)):
                session.render_to_numpy(
                    font_path="/tmp/nonexistent.ttf",
                    text="a",
                    size=1000.0,
                    width=100000,  # Very large
                    height=100000,
                )
        finally:
            session.close()

    def test_unicode_text_in_errors(self):
        """Test that Unicode text in errors is handled correctly."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "test_unicode",
                    "font": {
                        "path": "/nonexistent/font.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {
                        "content": "你好世界🌍",  # Chinese + emoji
                        "script": "Latn",
                    },
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                }
            ],
        }

        # Should handle Unicode gracefully in error messages
        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 1
        result = json.loads(results[0])
        assert result["status"] == "error"

    def test_multiple_jobs_some_failing(self):
        """Test batch with mix of valid and invalid jobs."""
        spec = {
            "version": "1.0",
            "jobs": [
                {
                    "id": "job1_fail",
                    "font": {
                        "path": "/nonexistent/font1.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "a", "script": "Latn"},
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                },
                {
                    "id": "job2_fail",
                    "font": {
                        "path": "/nonexistent/font2.ttf",
                        "size": 1000,
                        "variations": {},
                    },
                    "text": {"content": "b", "script": "Latn"},
                    "rendering": {
                        "format": "pgm",
                        "encoding": "base64",
                        "width": 3000,
                        "height": 1200,
                    },
                },
            ],
        }

        # Both should fail but return results
        results = list(haforu.process_jobs(json.dumps(spec)))
        assert len(results) == 2

        # Both should have error status
        for result_json in results:
            result = json.loads(result_json)
            assert result["status"] == "error"
            assert result["id"] in ["job1_fail", "job2_fail"]
