# this_file: reference/renderers/harfbuzz.py
"""
Renderer using HarfBuzz for shaping and FreeType for rasterisation.
"""

from __future__ import annotations

from pathlib import Path

import numpy as np

from .base import BaseRenderer, RendererInitError
from .constants import DEFAULT_FONT_SIZE, RENDER_BASELINE_RATIO, RENDER_HEIGHT, RENDER_WIDTH

try:
    import uharfbuzz as hb
    from freetype import Face, FT_LOAD_RENDER
    from freetype.raw import FT_Fixed, FT_Set_Var_Design_Coordinates
except ImportError as exc:  # pragma: no cover - dependency error handled via is_available
    HB_IMPORT_ERROR: ImportError | None = exc
else:
    HB_IMPORT_ERROR = None


class HarfBuzzRenderer(BaseRenderer):
    """
    Reference renderer based on the previous FreeType + HarfBuzz pipeline.
    """

    engine = "harfbuzz"

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
        if HB_IMPORT_ERROR:
            raise RendererInitError(
                f"harfbuzz renderer unavailable: {HB_IMPORT_ERROR}"
            ) from HB_IMPORT_ERROR

        super().__init__(
            font_path,
            instance_coords=instance_coords,
            features=features,
            width=width,
            height=height,
            font_size=font_size,
            tracking=tracking,
        )

        with open(font_path, "rb") as f:
            fontdata = f.read()

        self.hb_blob = hb.Blob(fontdata)
        self.hb_face = hb.Face(self.hb_blob)
        self.hb_font = hb.Font(self.hb_face)
        hb.ot_font_set_funcs(self.hb_font)

        upem = self.hb_face.upem
        self.hb_font.scale = (upem, upem)

        if instance_coords:
            try:
                self.hb_font.set_variations(instance_coords)
            except Exception as exc:  # pragma: no cover - upstream handles logging
                raise RendererInitError(f"Failed to set HarfBuzz variations: {exc}") from exc

        self.ft_face = Face(str(font_path))
        self.ft_face.set_pixel_sizes(0, font_size)

        if instance_coords:
            self._apply_variations_to_freetype()

    @classmethod
    def is_available(cls) -> bool:
        """Check if HarfBuzz renderer is available (requires uharfbuzz and freetype-py)."""
        return HB_IMPORT_ERROR is None

    def _apply_variations_to_freetype(self) -> None:
        variation_info = self.ft_face.get_variation_info()
        if not variation_info or not variation_info.axes:
            return

        # Build coordinates array preserving axis order from the font
        coords = []
        for axis in variation_info.axes:
            # Get the user coordinate value (e.g., wght=820)
            user_coord = self.instance_coords.get(axis.tag, axis.default)

            # Convert to FT_Fixed format (16.16 fixed-point)
            # This is what fontdiffenator does successfully
            coord = FT_Fixed(int(user_coord) << 16)
            coords.append(coord)

        if coords:
            # Create FT_Fixed array and call FT_Set_Var_Design_Coordinates
            ft_coords = (FT_Fixed * len(coords))(*coords)
            FT_Set_Var_Design_Coordinates(self.ft_face._FT_Face, len(ft_coords), ft_coords)

    def render_text(self, text: str) -> np.ndarray:
        """
        Render text using HarfBuzz for shaping and FreeType for rasterization.

        Args:
            text: Text string to render

        Returns:
            2D numpy array with rendered text (grayscale, 0=black, 255=white)
        """
        img = np.ones((self.height, self.width), dtype=np.uint8) * 255

        buf = hb.Buffer()
        buf.add_str(text)
        buf.guess_segment_properties()

        if self.features:
            hb.shape(
                self.hb_font,
                buf,
                features=[
                    hb.Feature.from_string(f"{tag}={value}") for tag, value in self.features.items()
                ],
            )
        else:
            hb.shape(self.hb_font, buf)

        infos = buf.glyph_infos
        positions = buf.glyph_positions

        # Handle empty string: HarfBuzz returns None for positions when text is empty
        if not text or positions is None:
            return img

        pen_x = 10.0
        pen_y = self.height * RENDER_BASELINE_RATIO
        scale = self.font_size / self.hb_face.upem

        tracking_px = float(self.tracking) / 1000.0 * float(self.font_size)
        for i, (info, pos) in enumerate(zip(infos, positions)):
            try:
                self.ft_face.load_glyph(info.codepoint, FT_LOAD_RENDER)
            except Exception as exc:  # pragma: no cover - dependent on font
                raise RendererInitError(f"Failed to load glyph {info.codepoint}: {exc}") from exc

            bitmap = self.ft_face.glyph.bitmap
            if not (bitmap.buffer and bitmap.width > 0 and bitmap.rows > 0):
                pen_x += pos.x_advance * scale
                continue

            glyph_array = np.array(bitmap.buffer, dtype=np.uint8).reshape(bitmap.rows, bitmap.width)

            x = pen_x + (pos.x_offset * scale) + self.ft_face.glyph.bitmap_left
            y = pen_y - self.ft_face.glyph.bitmap_top - (pos.y_offset * scale)
            x = int(round(x))
            y = int(round(y))

            self._composite_glyph(img, glyph_array, x, y)
            pen_x += pos.x_advance * scale
            # Apply tracking after each glyph to increase inter-glyph spacing
            if tracking_px:
                pen_x += tracking_px

        return img

    def _composite_glyph(self, img: np.ndarray, glyph: np.ndarray, x: int, y: int) -> None:
        gh, gw = glyph.shape
        ih, iw = img.shape

        x1 = max(0, x)
        y1 = max(0, y)
        x2 = min(iw, x + gw)
        y2 = min(ih, y + gh)

        if x2 <= x1 or y2 <= y1:
            return

        gx1 = max(0, -x)
        gy1 = max(0, -y)
        gx2 = gx1 + (x2 - x1)
        gy2 = gy1 + (y2 - y1)

        glyph_region = glyph[gy1:gy2, gx1:gx2].astype(np.float32) / 255.0
        if np.all(glyph_region == 0):
            return

        img_slice = img[y1:y2, x1:x2].astype(np.float32)
        img[y1:y2, x1:x2] = (img_slice * (1 - glyph_region)).astype(np.uint8)

    # --- Pooling update hooks ---
    def update_instance_coords(self, instance_coords: dict[str, float] | None) -> None:  # type: ignore[override]
        """
        Update variable font coordinates for both HarfBuzz and FreeType.

        Args:
            instance_coords: Dictionary of axis tags to coordinate values
        """
        self.instance_coords = instance_coords or {}
        try:
            # Update HarfBuzz font variations
            self.hb_font.set_variations(self.instance_coords)
        except Exception:
            # Keep previous variations if update fails
            pass
        # Update FreeType variation design coordinates
        try:
            self._apply_variations_to_freetype()
        except Exception:
            pass

    def update_dimensions(  # type: ignore[override]
        self,
        *,
        width: int | None = None,
        height: int | None = None,
        font_size: int | None = None,
    ) -> None:
        """
        Update renderer canvas dimensions and/or font size.

        Args:
            width: New canvas width in pixels (optional)
            height: New canvas height in pixels (optional)
            font_size: New font size in pixels (optional)

        Note:
            When font_size changes, updates the FreeType face pixel size.
        """
        if width is not None:
            self.width = int(width)
        if height is not None:
            self.height = int(height)
        if font_size is not None and int(font_size) != self.font_size:
            self.font_size = int(font_size)
            try:
                self.ft_face.set_pixel_sizes(0, self.font_size)
            except Exception:
                pass
