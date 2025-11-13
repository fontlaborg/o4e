# this_file: reference/renderers/coretext.py
"""
macOS CoreText renderer using PyObjC bindings.
"""

from __future__ import annotations

import platform
from pathlib import Path

import numpy as np

from .base import BaseRenderer, RendererInitError, RendererUnavailableError
from .constants import RENDER_BASELINE_RATIO

try:
    import Quartz  # type: ignore
    import CoreText  # type: ignore
    import CoreFoundation  # type: ignore
except ImportError as exc:  # pragma: no cover
    CORETEXT_IMPORT_ERROR: ImportError | None = exc
else:
    CORETEXT_IMPORT_ERROR = None


class CoreTextRenderer(BaseRenderer):
    """
    Hardware-accelerated renderer that delegates shaping and rasterisation to CoreText.
    """

    engine = "coretext"

    def __init__(
        self,
        font_path: Path,
        *,
        instance_coords: dict[str, float] | None = None,
        features: dict[str, int] | None = None,
        tracking: float = 0.0,
        **kwargs,
    ):
        if not self.is_available():
            raise RendererUnavailableError(
                "CoreText renderer requires macOS with pyobjc-framework-CoreText installed."
            )

        super().__init__(
            font_path,
            instance_coords=instance_coords,
            features=features,
            tracking=tracking,
            **kwargs,
        )

        # Cache font bytes to avoid disk I/O churn on variation updates
        with open(font_path, "rb") as f:
            self._font_bytes = f.read()
        self._font = self._create_font(font_path, self.font_size, instance_coords, features)

    @classmethod
    def is_available(cls) -> bool:
        """Check if CoreText renderer is available (macOS only)."""
        return platform.system() == "Darwin" and CORETEXT_IMPORT_ERROR is None

    def render_text(self, text: str) -> np.ndarray:
        """
        Render text using macOS CoreText framework.

        Args:
            text: Text string to render

        Returns:
            2D numpy array with rendered text (grayscale, 0=black, 255=white)
        """
        width = self.width
        height = self.height

        color_space = Quartz.CGColorSpaceCreateDeviceGray()
        context = Quartz.CGBitmapContextCreate(
            None,
            width,
            height,
            8,
            width,
            color_space,
            Quartz.kCGImageAlphaNone,
        )
        if context is None:
            raise RendererInitError("Failed to create CoreGraphics context")

        # Background white
        Quartz.CGContextSetGrayFillColor(context, 1.0, 1.0)
        Quartz.CGContextFillRect(context, Quartz.CGRectMake(0, 0, width, height))

        # Prepare for text drawing
        # Note on coordinates:
        # - CGBitmapContext memory layout is top-to-bottom (row 0 is the top).
        # - CoreGraphics' default user space has origin at bottom-left with +Y upwards.
        # If we flip the CTM (translate+scale) before drawing, the pixels end up inverted
        # when read back as a numpy array (which assumes row 0 is the top). To keep the
        # output consistent with other engines (Pillow/Skia), we draw in the default
        # coordinate space (no CTM flip) and compute the baseline from the bottom.
        Quartz.CGContextSetGrayFillColor(context, 0.0, 1.0)
        Quartz.CGContextSetTextMatrix(context, Quartz.CGAffineTransformIdentity)

        attrs = {CoreText.kCTFontAttributeName: self._font}
        # Apply tracking via CoreText's kerning attribute. The expected unit is in points.
        # We convert 1/1000 em to pixels/points by multiplying by font_size.
        try:
            kern_points = float(self.tracking) / 1000.0 * float(self.font_size)
            if abs(kern_points) > 1e-9:
                attrs[CoreText.kCTKernAttributeName] = kern_points
        except Exception:
            # If CoreText constants or types are unavailable, silently ignore tracking.
            pass
        attr_string = CoreFoundation.CFAttributedStringCreate(
            None,
            text,
            attrs,
        )
        line = CoreText.CTLineCreateWithAttributedString(attr_string)

        # Compute baseline from the BOTTOM of the image because CoreGraphics' default
        # coordinate system has origin at the bottom-left. This matches the visual
        # placement used by other backends that measure baseline from the top.
        baseline_y = int(height * (1.0 - RENDER_BASELINE_RATIO))
        Quartz.CGContextSetTextPosition(context, 10, baseline_y)
        CoreText.CTLineDraw(line, context)

        image = Quartz.CGBitmapContextCreateImage(context)
        if image is None:
            raise RendererInitError("Failed to create image from CoreGraphics context")

        provider = Quartz.CGImageGetDataProvider(image)
        data = Quartz.CGDataProviderCopyData(provider)
        length = CoreFoundation.CFDataGetLength(data)
        byte_buffer = bytearray(length)
        CoreFoundation.CFDataGetBytes(data, (0, length), byte_buffer)
        buffer = np.frombuffer(byte_buffer, dtype=np.uint8, count=length)
        return buffer.reshape(height, width).copy()

    def _create_font(
        self,
        font_path: Path,
        font_size: int,
        instance_coords: dict[str, float] | None,
        features: dict[str, int] | None,
    ):
        # Load font data from cached bytes when available to avoid repeated I/O
        try:
            font_data = getattr(self, "_font_bytes")
        except Exception:
            with open(font_path, "rb") as f:
                font_data = f.read()

        cf_data = CoreFoundation.CFDataCreate(None, font_data, len(font_data))
        data_provider = Quartz.CGDataProviderCreateWithCFData(cf_data)
        cg_font = Quartz.CGFontCreateWithDataProvider(data_provider)

        if not cg_font:
            raise RendererInitError(f"Failed to load font from {font_path}")

        # Create CTFont from CGFont
        base_font = CoreText.CTFontCreateWithGraphicsFont(cg_font, font_size, None, None)

        # If no variations needed, return the base font
        if not instance_coords and not features:
            return base_font

        # Build attributes for font with variations
        attrs: dict = {}

        if instance_coords:
            # CoreText expects axis IDs as integers, not strings
            # Convert standard axis tags to their numeric IDs
            axis_ids = {
                "wght": 2003265652,  # kCTFontWeightTrait equivalent
                "wdth": 2003072104,  # kCTFontWidthTrait equivalent
                "slnt": 1936486004,  # Slant axis
                "ital": 1769234796,  # Italic axis
                "opsz": 1869640570,  # Optical size
            }

            variation_dict: dict[int | bytes, float] = {}
            for tag, value in instance_coords.items():
                if tag in axis_ids:
                    # Use numeric axis ID
                    variation_dict[axis_ids[tag]] = float(value)
                else:
                    # Try using the string tag directly (may not work for all axes)
                    import struct

                    try:
                        # Convert 4-character tag to integer
                        tag_int = struct.unpack("!I", tag.encode("utf-8")[:4].ljust(4, b" "))[0]
                        variation_dict[tag_int] = float(value)
                    except (struct.error, UnicodeEncodeError):
                        # Fall back to string (likely won't work but worth trying)
                        variation_dict[tag.encode("utf-8")] = float(value)

            attrs[CoreText.kCTFontVariationAttribute] = variation_dict

        if features:
            feature_settings = []
            for tag, selector in features.items():
                feature_settings.append(
                    {
                        CoreText.kCTFontFeatureTypeIdentifierKey: tag,
                        CoreText.kCTFontFeatureSelectorIdentifierKey: selector,
                    }
                )
            attrs[CoreText.kCTFontFeatureSettingsAttribute] = feature_settings

        if attrs:
            # Create font descriptor with variations/features
            descriptor = CoreText.CTFontDescriptorCreateWithAttributes(attrs)
            # Create a new font with the variations applied
            return CoreText.CTFontCreateCopyWithAttributes(base_font, font_size, None, descriptor)
        else:
            return base_font

    # --- Pooling update hooks ---
    def update_instance_coords(self, instance_coords: dict[str, float] | None) -> None:  # type: ignore[override]
        """
        Update variable font coordinates and rebuild CTFont.

        Args:
            instance_coords: Dictionary of axis tags to coordinate values
        """
        self.instance_coords = instance_coords or {}
        try:
            # Rebuild CTFont with new variation coordinates
            self._font = self._create_font(
                self.font_path, self.font_size, self.instance_coords, self.features
            )
        except Exception:
            # Keep previous font on failure
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
            font_size: New font size in points (optional)

        Note:
            When font_size changes, rebuilds the CTFont to reflect the new size.
        """
        if width is not None:
            self.width = int(width)
        if height is not None:
            self.height = int(height)
        if font_size is not None and int(font_size) != self.font_size:
            self.font_size = int(font_size)
            try:
                # Rebuild CTFont to reflect new size
                self._font = self._create_font(
                    self.font_path, self.font_size, self.instance_coords, self.features
                )
            except Exception:
                pass
