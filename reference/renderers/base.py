# this_file: reference/renderers/base.py
"""
Base abstractions for renderer backends.
"""

from __future__ import annotations

from abc import ABC, abstractmethod
from pathlib import Path
from typing import Any

import numpy as np

from .constants import DEFAULT_FONT_SIZE, RENDER_HEIGHT, RENDER_WIDTH


class RendererInitError(RuntimeError):
    """Raised when a renderer cannot be constructed."""


class RendererUnavailableError(RuntimeError):
    """Raised when an engine is not available on the current platform."""


class BaseRenderer(ABC):
    """
    Abstract base class for renderer backends.
    """

    engine: str = "base"

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
        self.font_path = font_path
        self.instance_coords = instance_coords or {}
        self.features = features or {}
        self.width = width
        self.height = height
        self.font_size = font_size
        # Tracking value in 1/1000 em units (CSS-style)
        # Effective added spacing per glyph in pixels is: tracking/1000 * font_size
        self.tracking = float(tracking)

    @classmethod
    def is_available(cls) -> bool:
        """
        Return True if the engine can be constructed on the current system.
        """
        return True

    @abstractmethod
    def render_text(self, text: str) -> np.ndarray:
        """
        Render text to a grayscale numpy array.
        """

    def save_image(self, img: np.ndarray, output_path: Path) -> None:
        """
        Save rendered image to disk using OpenCV.
        """
        try:
            import cv2
        except ImportError as exc:
            msg = "OpenCV (cv2) is required for saving rendered images."
            raise RendererInitError(msg) from exc

        # Ensure parent directory exists (robustness for nested outputs)
        try:
            output_path.parent.mkdir(parents=True, exist_ok=True)
        except Exception as exc:  # pragma: no cover - defensive
            raise RendererInitError(
                f"Failed to create output directory: {output_path.parent}"
            ) from exc

        if not cv2.imwrite(str(output_path), img):
            raise RendererInitError(f"Failed to save image to {output_path}")

    def summary(self) -> dict[str, Any]:
        """
        Diagnostics for benchmarking.
        """
        return {
            "engine": self.engine,
            "font": self.font_path.name,
            "coords": self.instance_coords,
            "features": self.features,
            "size": self.font_size,
        }

    # --- Mutable update hooks for pooling ---
    def update_instance_coords(self, instance_coords: dict[str, float] | None) -> None:
        """
        Update variation coordinates on an existing renderer instance.

        Default behavior only updates the stored attributes. Backends that
        need to propagate variations to native objects should override.
        """
        self.instance_coords = instance_coords or {}

    def update_tracking(self, tracking: float | int | None) -> None:
        """Update tracking in 1/1000 em units."""
        try:
            self.tracking = float(tracking or 0.0)
        except Exception:
            self.tracking = 0.0

    def update_dimensions(
        self,
        *,
        width: int | None = None,
        height: int | None = None,
        font_size: int | None = None,
    ) -> None:
        """
        Update canvas dimensions and/or font size.

        Default behavior updates attributes. Backends that must reconfigure
        native resources (e.g., FreeType size) should override.
        """
        if width is not None:
            self.width = int(width)
        if height is not None:
            self.height = int(height)
        if font_size is not None:
            self.font_size = int(font_size)
