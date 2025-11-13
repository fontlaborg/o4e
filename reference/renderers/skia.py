# this_file: reference/renderers/skia.py
"""
Skia-based renderer via skia-python.
"""

from __future__ import annotations

from pathlib import Path
import struct

import numpy as np
from loguru import logger

from .base import BaseRenderer, RendererInitError
from .constants import RENDER_BASELINE_RATIO

try:
    import skia
except ImportError as exc:  # pragma: no cover - optional dependency
    SKIA_IMPORT_ERROR: ImportError | None = exc
else:
    SKIA_IMPORT_ERROR = None


class SkiaRenderer(BaseRenderer):
    """
    High performance renderer backed by Skia's text stack.
    """

    engine = "skia"

    def __init__(
        self,
        font_path: Path,
        *,
        instance_coords: dict[str, float] | None = None,
        features: dict[str, int] | None = None,
        **kwargs,
    ):
        if SKIA_IMPORT_ERROR:
            raise RendererInitError(
                f"skia renderer unavailable: {SKIA_IMPORT_ERROR}"
            ) from SKIA_IMPORT_ERROR

        super().__init__(
            font_path,
            instance_coords=instance_coords,
            features=features,
            **kwargs,
        )

        self._font_mgr = skia.FontMgr.RefDefault()
        self._typeface = self._load_typeface(self.font_path, instance_coords)
        if self._typeface is None:
            raise RendererInitError(f"Failed to load typeface from {font_path}")

        self._font = skia.Font(self._typeface, self.font_size)
        if features:
            font_features = []
            for tag, value in features.items():
                try:
                    font_features.append(skia.FontFeature(skia.FourByteTag(tag), int(value)))
                except Exception:  # pragma: no cover - invalid feature tag
                    continue
            if font_features:
                self._font.setFeatures(font_features)

        self._paint = skia.Paint(
            Color=skia.ColorBLACK,
            AntiAlias=True,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Check if Skia renderer is available (requires skia-python)."""
        return SKIA_IMPORT_ERROR is None

    def render_text(self, text: str) -> np.ndarray:
        """
        Render text using Skia graphics library.

        Args:
            text: Text string to render

        Returns:
            2D numpy array with rendered text (grayscale, 0=black, 255=white)
        """
        surface = skia.Surface(self.width, self.height)
        canvas = surface.getCanvas()
        canvas.clear(skia.ColorWHITE)

        baseline = int(self.height * RENDER_BASELINE_RATIO)

        tracking_px = float(self.tracking) / 1000.0 * float(self.font_size)

        if abs(tracking_px) < 1e-9:
            # Fast path: draw as a single blob to retain kerning
            blob = skia.TextBlob.MakeFromText(text, self._font, skia.TextEncoding.kUTF8)
            canvas.drawTextBlob(blob, 10, baseline, self._paint)
        else:
            # Draw per-glyph with additional tracking between glyphs
            pen_x = float(10)
            for ch in text:
                blob = skia.TextBlob.MakeFromText(ch, self._font, skia.TextEncoding.kUTF8)
                canvas.drawTextBlob(blob, pen_x, baseline, self._paint)
                try:
                    advance = float(self._font.measureText(ch, skia.TextEncoding.kUTF8))
                except Exception:
                    advance = 0.0
                pen_x += advance + tracking_px

        snapshot = surface.makeImageSnapshot()
        rgba = snapshot.toarray()
        # Convert RGBA to grayscale by taking the red channel (all equal for grayscale drawing)
        gray = rgba[:, :, 0]
        return gray.copy()

    def _load_typeface(
        self,
        font_path: Path,
        instance_coords: dict[str, float] | None,
    ):
        # Load the base typeface from file
        base_typeface = skia.Typeface.MakeFromFile(str(font_path))
        if not base_typeface:
            return None

        if instance_coords:
            try:
                # Convert tag strings to 4-byte integers as pictex does
                def to_four_char_code(tag):
                    """Convert OpenType tag string to 4-byte integer code."""
                    return struct.unpack("!I", tag.encode("utf-8"))[0]

                # Create coordinate objects for each axis
                coords_list = [
                    skia.FontArguments.VariationPosition.Coordinate(
                        axis=to_four_char_code(tag), value=float(value)
                    )
                    for tag, value in instance_coords.items()
                ]

                # Wrap coordinates in Coordinates object as pictex does
                coordinates = skia.FontArguments.VariationPosition.Coordinates(coords_list)
                variation_position = skia.FontArguments.VariationPosition(coordinates)

                # Create FontArguments with variation position
                args = skia.FontArguments()
                args.setVariationDesignPosition(variation_position)

                # Clone the typeface with the variations applied
                return base_typeface.makeClone(args)
            except Exception as exc:  # pragma: no cover - invalid axis tags fallback
                # Log the error for debugging
                logger.warning(f"Failed to set variations: {exc}")
                return base_typeface
        return base_typeface
