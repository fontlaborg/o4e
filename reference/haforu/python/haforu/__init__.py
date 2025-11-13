# this_file: python/haforu/__init__.py

"""Haforu: High-performance batch font renderer.

This package provides Python bindings to the haforu Rust library for
fast batch font rendering with zero-copy numpy array integration.

Example:
    >>> import haforu
    >>> print(haforu.__version__)
    2.0.0
"""

from __future__ import annotations

try:
    from haforu._haforu import (
        __version__,
        __doc__,
        process_jobs,
        StreamingSession,
        is_available,
    )
except ImportError as e:
    raise ImportError(
        "Failed to import haforu._haforu extension module. "
        "Make sure haforu is properly installed with: pip install haforu"
    ) from e

__all__ = [
    "__version__",
    "process_jobs",
    "StreamingSession",
    "is_available",
]
