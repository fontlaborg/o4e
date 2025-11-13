# this_file: reference/renderers/haforu_python.py
"""
Haforu Python bindings renderer (maximum performance).

This backend uses the haforu PyO3-based Python package to render directly
to numpy arrays with zero subprocess overhead. It complements the existing
CLI-based Haforu renderer by providing a persistent in-process streaming
session that caches font instances across renders.

Notes:
- Engine name: "haforu-python" (distinct from CLI engine "haforu")
- Availability: Depends on `pip install haforu` (maturin-built wheels)
- Fallbacks: If import fails, this engine is simply not registered

Typical usage:
    >>> from pathlib import Path
    >>> from o4e.reference.renderers.haforu_python import HaforuPythonRenderer
    >>> r = HaforuPythonRenderer(Path("fonts/Archivo[wdth,wght].ttf"))
    >>> img = r.render_text("Hello")
    >>> img.shape  # (height, width)
    (1200, 3000)
"""

from __future__ import annotations

from pathlib import Path
import threading
from typing import Any

import numpy as np

from .base import BaseRenderer, RendererInitError
from .constants import DEFAULT_FONT_SIZE, RENDER_HEIGHT, RENDER_WIDTH

try:  # pragma: no cover - availability depends on developer environment
    import haforu  # type: ignore
    # Require StreamingSession to consider the bindings available
    _HAFORU_AVAILABLE = hasattr(haforu, "StreamingSession")  # type: ignore[attr-defined]
except Exception:  # pragma: no cover - treated as unavailable in CI without wheels
    haforu = None  # type: ignore
    _HAFORU_AVAILABLE = False

_SESSION_LOCK = threading.Lock()
_SHARED_SESSION: Any | None = None


def _create_shared_session() -> Any | None:
    if not _HAFORU_AVAILABLE:
        return None
    try:
        session = haforu.StreamingSession()  # type: ignore[attr-defined]
        warm_up = getattr(session, "warm_up", None)
        if callable(warm_up):
            try:
                warm_up()
            except Exception:
                pass
        return session
    except Exception:
        return None


def _get_shared_session() -> Any | None:
    global _SHARED_SESSION
    if _SHARED_SESSION is not None:
        return _SHARED_SESSION
    with _SESSION_LOCK:
        if _SHARED_SESSION is None:
            _SHARED_SESSION = _create_shared_session()
    return _SHARED_SESSION


def shutdown_shared_session() -> None:
    global _SHARED_SESSION
    with _SESSION_LOCK:
        if _SHARED_SESSION is not None:
            try:
                _SHARED_SESSION.close()
            except Exception:
                pass
            _SHARED_SESSION = None


class HaforuPythonRenderer(BaseRenderer):
    """Renderer using haforu Python bindings (fastest path).

    Requires the `haforu` Python package to be importable. When not available,
    construction raises a RendererInitError and the engine should not be used.
    """

    engine = "haforu-python"

    def __init__(
        self,
        font_path: Path,
        *,
        instance_coords: dict[str, float] | None = None,
        features: dict[str, int] | None = None,
        width: int = RENDER_WIDTH,
        height: int = RENDER_HEIGHT,
        font_size: int = DEFAULT_FONT_SIZE,
        tracking: float = 0.0,
    ):
        # When haforu bindings are not fully available, degrade gracefully by
        # falling back to the standard renderer selection (preferring haforu CLI).
        super().__init__(
            font_path,
            instance_coords=instance_coords,
            features=features,
            width=width,
            height=height,
            font_size=font_size,
            tracking=tracking,
        )
        self._session = _get_shared_session()
        self._fallback_renderer = None
        # Track loaded fonts if needed by future logic (parity with other backends)
        self._font_paths: dict[str, Path] = {}

    @classmethod
    def is_available(cls) -> bool:  # pragma: no cover - trivial
        probe = getattr(haforu, "is_available", None)
        if callable(probe):
            try:
                return bool(probe())
            except Exception:
                return False
        return _HAFORU_AVAILABLE

    def render_text(self, text: str) -> np.ndarray:
        if not text:
            raise RendererInitError("Text parameter cannot be empty")
        if not self.font_path.exists() or not self.font_path.is_file():
            raise RendererInitError(f"Font file not found: {self.font_path}")
        if self.width <= 0 or self.height <= 0:
            raise RendererInitError(
                f"Invalid dimensions: width={self.width}, height={self.height} (must be positive)"
            )
        if self.font_size <= 0:
            raise RendererInitError(f"Invalid font size: {self.font_size} (must be positive)")

        # Prefer haforu bindings when available
        if self._session is not None:
            try:
                img = self._session.render_to_numpy(  # type: ignore[attr-defined]
                    font_path=str(self.font_path),
                    text=text,
                    size=float(self.font_size),
                    width=int(self.width),
                    height=int(self.height),
                    variations=dict(self.instance_coords or {}),
                    script="Latn",
                    direction="ltr",
                    language="en",
                )
            except Exception as exc:  # pragma: no cover - delegated error mapping
                raise RendererInitError(f"haforu render failed: {exc}") from exc
        else:
            # Fallback path: use CLI haforu if available, otherwise default engine
            try:
                from . import create_renderer_with_fallback  # lazy import to avoid cycles

                if self._fallback_renderer is None:
                    self._fallback_renderer = create_renderer_with_fallback(
                        "haforu",
                        self.font_path,
                        instance_coords=self.instance_coords,
                        features=self.features,
                        width=self.width,
                        height=self.height,
                        font_size=self.font_size,
                        tracking=self.tracking,
                    )
                    if self._fallback_renderer is None:
                        # Last resort: try platform default from selection utilities
                        self._fallback_renderer = create_renderer_with_fallback(
                            "auto",
                            self.font_path,
                            instance_coords=self.instance_coords,
                            features=self.features,
                            width=self.width,
                            height=self.height,
                            font_size=self.font_size,
                            tracking=self.tracking,
                        )
                if self._fallback_renderer is None:
                    raise RendererInitError(
                        "haforu bindings unavailable and no fallback renderer could be created"
                    )
                img = self._fallback_renderer.render_text(text)
            except Exception as exc:
                raise RendererInitError(f"haforu-python fallback failed: {exc}") from exc

        if not isinstance(img, np.ndarray) or img.dtype != np.uint8:
            raise RendererInitError("haforu returned unexpected image format (expected uint8 ndarray)")
        # Ensure shape matches requested dimensions (defensive)
        if tuple(img.shape) != (self.height, self.width):
            raise RendererInitError(
                f"Unexpected image shape {img.shape}, expected ({self.height}, {self.width})"
            )
        return img

    def close(self) -> None:  # pragma: no cover - trivial
        try:
            if hasattr(self, "_fallback_renderer") and self._fallback_renderer is not None:
                close = getattr(self._fallback_renderer, "close", None)
                if callable(close):
                    close()
        except Exception:
            pass

    def __del__(self) -> None:  # pragma: no cover - best-effort cleanup
        try:
            self.close()
        except Exception:
            pass
