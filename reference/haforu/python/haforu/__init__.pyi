# this_file: python/haforu/__init__.pyi

"""Type stubs for haforu package."""

from __future__ import annotations
from typing import Iterator, Optional, Any

__version__: str

def is_available() -> bool:
    """Return True when haforu bindings and dependencies are ready."""
    ...

def process_jobs(spec_json: str) -> Iterator[str]:
    """Process a batch of rendering jobs in parallel.

    Args:
        spec_json: JSON string containing JobSpec with jobs array

    Returns:
        Iterator yielding JSONL result strings (one per completed job)

    Raises:
        ValueError: Invalid JSON or job specification
        RuntimeError: Font loading or rendering errors
    """
    ...

class StreamingSession:
    """Persistent rendering session with font cache.

    Maintains loaded fonts across multiple renders for maximum performance.
    Thread-safe: can be called from multiple threads concurrently.

    Example:
        >>> import haforu
        >>> import json
        >>>
        >>> with haforu.StreamingSession() as session:
        ...     job = {
        ...         "id": "test1",
        ...         "font": {"path": "/path/to/font.ttf", "size": 1000, "variations": {}},
        ...         "text": {"content": "a"},
        ...         "rendering": {"format": "pgm", "encoding": "base64", "width": 3000, "height": 1200}
        ...     }
        ...     result_json = session.render(json.dumps(job))
        ...     result = json.loads(result_json)
        ...     print(f"Status: {result['status']}")
    """

    @classmethod
    def is_available(cls) -> bool:
        """Cheap probe indicating whether StreamingSession can be constructed."""
        ...

    def __init__(self, cache_size: int = 512) -> None:
        """Create a new streaming session.

        Args:
            cache_size: Maximum number of fonts to keep in cache (default: 512)
        """
        ...

    def warm_up(
        self,
        font_path: str | None = None,
        *,
        text: str = "Haforu",
        size: float = 600.0,
        width: int = 128,
        height: int = 128,
    ) -> bool:
        """Prime caches (optionally rendering a font) for faster subsequent renders."""
        ...

    def cache_stats(self) -> dict[str, int]:
        """Return cache capacity and current entry count."""
        ...

    def set_cache_size(self, cache_size: int) -> None:
        """Resize cache capacity (drops cached entries)."""
        ...

    def render(self, job_json: str) -> str:
        """Render a single job and return JSONL result.

        Args:
            job_json: JSON string containing single Job specification

        Returns:
            JSONL result string with base64-encoded image

        Raises:
            ValueError: Invalid JSON or job specification
            RuntimeError: Font loading or rendering errors
        """
        ...

    def render_to_numpy(
        self,
        font_path: str,
        text: str,
        size: float,
        width: int,
        height: int,
        variations: dict[str, float] | None = None,
        script: str | None = None,
        direction: str | None = None,
        language: str | None = None,
    ) -> Any:  # numpy.ndarray[numpy.uint8]
        """Render text directly to numpy array (zero-copy).

        Args:
            font_path: Absolute path to font file
            text: Text to render (typically single glyph)
            size: Font size in points (typically 1000)
            width: Canvas width in pixels
            height: Canvas height in pixels
            variations: Variable font coordinates (e.g. {"wght": 600.0})
            script: Script tag (default: "Latn")
            direction: Text direction (default: "ltr")
            language: str | None (default: "en")

        Returns:
            2D numpy array of shape (height, width), dtype uint8
            Grayscale values 0-255

        Raises:
            ValueError: Invalid parameters
            RuntimeError: Font loading or rendering errors
        """
        ...

    def close(self) -> None:
        """Close session and release resources.

        Clears font cache and releases memory-mapped files.
        Session cannot be used after closing.
        """
        ...

    def __enter__(self) -> StreamingSession:
        """Enter context manager."""
        ...

    def __exit__(
        self,
        exc_type: Optional[type[BaseException]],
        exc_val: Optional[BaseException],
        exc_tb: Optional[Any],
    ) -> bool:
        """Exit context manager."""
        ...

__all__ = [
    "__version__",
    "is_available",
    "process_jobs",
    "StreamingSession",
]
