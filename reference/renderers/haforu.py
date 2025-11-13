# this_file: reference/renderers/haforu.py
"""
Haforu renderer backend.

This backend integrates with the external `haforu` binary for high-performance
glyph rendering via subprocess communication using JSON job specifications
and JSONL output.

Features:
- Single-call rendering via subprocess with JSON job spec
- PNG format output with automatic grayscale conversion
- Variable font coordinate support
- Comprehensive error detection and recovery
- Temporary directory management with automatic cleanup

Binary Discovery (in order):
- Environment variable `HAFORU_BIN` (takes precedence)
- Repository-relative paths: `external/haforu2/target/release/haforu2`, then `external/haforu/target/release/haforu`
- On PATH: `haforu2` then `haforu`

Performance Notes:
- Each render_text() call spawns a new haforu subprocess
- Subprocess overhead: ~50-100ms per call
- Suitable for individual renders; batch mode planned for Phase H2
- For high-throughput scenarios (1000+ renders), consider CoreText/HarfBuzz

Temporary Files:
- Creates temp directory per render_text() call
- Haforu writes PNG to temp dir
- Automatic cleanup on success or failure (best effort)

Update Methods and Renderer Pooling:
    HaforuRenderer supports the standard BaseRenderer update methods for
    changing rendering parameters without creating new renderer instances.
    Unlike CoreText/HarfBuzz renderers that update native font objects,
    HaforuRenderer's updates only modify Python attributes.

    Each render_text() call spawns a fresh haforu subprocess using the
    current attribute values, so updates take effect immediately in the
    next render_text() call. There is no native object state to maintain.

    This subprocess-per-call architecture means:
    - Updates are simple attribute assignments (no native object lifecycle)
    - Each render gets a fresh subprocess with current parameters
    - No memory leaks from native object accumulation
    - Slightly higher per-call overhead vs persistent process (Phase H3)

    Example - Using update methods:
        >>> renderer = HaforuRenderer(
        ...     Path("fonts/Archivo[wdth,wght].ttf"),
        ...     instance_coords={"wght": 400},
        ...     width=1200, height=200, font_size=72
        ... )
        >>> img1 = renderer.render_text("Regular")
        >>>
        >>> # Update to bold with different dimensions
        >>> renderer.update_instance_coords({"wght": 700})
        >>> renderer.update_dimensions(font_size=96)
        >>> img2 = renderer.render_text("Bold")  # Uses wght=700, size=96

Example Usage:
    >>> from pathlib import Path
    >>> from o4e.reference.renderers.haforu import HaforuRenderer
    >>>
    >>> # Basic usage
    >>> renderer = HaforuRenderer(
    ...     Path("fonts/Archivo[wdth,wght].ttf"),
    ...     width=1200,
    ...     height=200,
    ...     font_size=72
    ... )
    >>> img = renderer.render_text("Hello World")
    >>> print(img.shape, img.dtype)  # (200, 1200) uint8
    >>>
    >>> # With variable font coordinates
    >>> renderer = HaforuRenderer(
    ...     Path("fonts/Archivo[wdth,wght].ttf"),
    ...     instance_coords={"wght": 500, "wdth": 125},
    ...     font_size=96
    ... )
    >>> img = renderer.render_text("Variable Font")

Current Limitations:
- Haforu Rust rendering not yet implemented (returns "pending" status)
- Falls back to other engines when using --renderer=auto
- Ready to use immediately when haforu Rust code is complete

See Also:
- Phase H2: Batch rendering (5000+ jobs in single subprocess call)
- Phase H3: Streaming mode (persistent process for optimization)
- docs/haforu-usage.md: Installation and troubleshooting
"""

from __future__ import annotations

import base64
import json
import subprocess
from pathlib import Path
from typing import Final

import numpy as np

from .base import BaseRenderer, RendererInitError, RendererUnavailableError
from .constants import DEFAULT_FONT_SIZE, RENDER_HEIGHT, RENDER_WIDTH


_ENV_BIN: Final[str] = "HAFORU_BIN"
_DEFAULT_RELATIVE_BIN: Final[Path] = Path("external/haforu/target/release/haforu")
_DEFAULT_RELATIVE_BIN_V2: Final[Path] = Path("external/haforu2/target/release/haforu2")


def _resolve_haforu_bin() -> Path | None:
    """Resolve the path to the haforu binary if present.

    Search order:
    1) Environment variable `HAFORU_BIN`
    2) Repository-relative defaults (haforu2, then haforu)
    3) Executable found on PATH (`haforu2`, then `haforu`)
    """
    import os

    env_path = os.environ.get(_ENV_BIN)
    if env_path:
        p = Path(env_path).expanduser().resolve()
        if p.exists() and p.is_file():
            return p

    # Fallback to repo-relative defaults (prefer haforu2 if present)
    repo_root = Path.cwd()
    candidate_v2 = (repo_root / _DEFAULT_RELATIVE_BIN_V2).resolve()
    if candidate_v2.exists() and candidate_v2.is_file():
        return candidate_v2

    candidate = (repo_root / _DEFAULT_RELATIVE_BIN).resolve()
    if candidate.exists() and candidate.is_file():
        return candidate

    # As a last attempt, try to find on PATH
    from shutil import which

    for name in ("haforu2", "haforu"):
        exe = which(name)
        if exe:
            return Path(exe)
    return None


def _decode_pgm_base64(data: str, width: int, height: int) -> np.ndarray:
    """Decode base64-encoded PGM binary data to numpy array.

    Expects P5 (binary PGM) format with header stripped and only pixel data.
    Returns (height, width) uint8 grayscale array.
    """
    try:
        # Decode base64 to raw bytes
        raw_bytes = base64.b64decode(data)

        # PGM P5 binary format: "P5\nwidth height\nmaxval\n" + raw pixel data
        # Haforu should return just the pixel data portion after base64 encoding
        pixel_data = np.frombuffer(raw_bytes, dtype=np.uint8)

        # Reshape to (height, width)
        if len(pixel_data) != width * height:
            raise ValueError(
                f"PGM data size mismatch: expected {width}×{height}={width * height} bytes, "
                f"got {len(pixel_data)} bytes"
            )

        return pixel_data.reshape((height, width))
    except Exception as exc:
        raise RendererInitError(f"Failed to decode PGM data: {exc}") from exc


class HaforuRenderer(BaseRenderer):
    """Haforu renderer backend using subprocess communication.

    Each render_text() call spawns a haforu subprocess with a JSON job spec
    and parses the JSONL output to extract the rendered image.
    """

    engine = "haforu"

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
        """Initialize Haforu renderer.

        Verifies haforu binary is available before proceeding.
        """
        # Check binary availability
        self._haforu_bin = _resolve_haforu_bin()
        if self._haforu_bin is None:
            # Build detailed error message with search paths
            import os

            repo_root = Path.cwd()
            searched_paths = [
                f"  • HAFORU_BIN environment variable: {os.environ.get(_ENV_BIN, 'not set')}",
                f"  • {repo_root / _DEFAULT_RELATIVE_BIN_V2} (haforu2)",
                f"  • {repo_root / _DEFAULT_RELATIVE_BIN} (haforu)",
                "  • haforu2 on PATH",
                "  • haforu on PATH",
            ]

            error_msg = (
                "Haforu binary not found.\n\n"
                "Searched locations:\n" + "\n".join(searched_paths) + "\n\n"
                "To enable Haforu:\n"
                "  1. Build: cd haforu2 && cargo build --release\n"
                "  2. Setup: source scripts/setup_haforu_env.sh\n"
                "  3. Verify renderer availability\n\n"
                "For more details, see: docs/haforu-usage.md"
            )
            raise RendererUnavailableError(error_msg)

        super().__init__(
            font_path,
            instance_coords=instance_coords,
            features=features,
            width=width,
            height=height,
            font_size=font_size,
            tracking=tracking,
        )

    @classmethod
    def is_available(cls) -> bool:
        """Return True if the haforu binary is discoverable."""
        return _resolve_haforu_bin() is not None

    def render_text(self, text: str) -> np.ndarray:
        """Render text to grayscale numpy array via haforu subprocess.

        Generates a JSON job specification, calls haforu process command,
        parses JSONL output, and decodes the base64 PGM image data.

        Args:
            text: Text string to render (1-10000 characters)

        Returns:
            (height, width) uint8 grayscale array

        Raises:
            RendererInitError: If text is invalid, subprocess fails, or output cannot be parsed
            RendererUnavailableError: If haforu rendering is not yet implemented
        """
        # Input validation
        if not text:
            raise RendererInitError("Text parameter cannot be empty")

        if len(text) > 10000:
            raise RendererInitError(
                f"Text parameter too long: {len(text)} characters (maximum: 10000)"
            )

        if not self.font_path.exists():
            raise RendererInitError(f"Font file not found: {self.font_path}")

        if not self.font_path.is_file():
            raise RendererInitError(f"Font path is not a file: {self.font_path}")

        if self.width <= 0 or self.height <= 0:
            raise RendererInitError(
                f"Invalid dimensions: width={self.width}, height={self.height} "
                "(both must be positive integers)"
            )

        if self.font_size <= 0:
            raise RendererInitError(
                f"Invalid font size: {self.font_size} (must be positive integer)"
            )

        # Generate job ID for tracking
        job_id = f"render_{hash(text) & 0xFFFFFFFF:08x}"

        # Build JSON job specification
        import tempfile

        # Create temp dir for haforu output (cleanup ensured via finally block)
        temp_dir = tempfile.mkdtemp(prefix="haforu_render_")

        job_spec = {
            "version": "1.0.0",
            "jobs": [
                {
                    "id": job_id,
                    "font": {
                        "path": str(self.font_path.absolute()),
                        "variations": [
                            {"tag": tag, "value": value}
                            for tag, value in self.instance_coords.items()
                        ]
                        if self.instance_coords
                        else [],
                    },
                    "text": text,
                    "size": float(self.font_size),
                    "shaping": {
                        "direction": "ltr",
                        "language": "en",
                    },
                    "rendering": {
                        "enabled": True,
                        "format": "png",  # Use PNG for now, convert to grayscale
                        "width": self.width,
                        "height": self.height,
                    },
                }
            ],
            "storage": {
                "backend": "filesystem",
                "output_path": temp_dir,
                "compress": False,
            },
            "include_shaping_output": False,
        }

        # Serialize to JSON
        job_json = json.dumps(job_spec)

        # Call haforu process command and parse output
        try:
            try:
                result = subprocess.run(
                    [str(self._haforu_bin), "process"],
                    input=job_json,
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=30,  # 30 second timeout for rendering
                )
            except subprocess.TimeoutExpired as exc:
                raise RendererInitError(
                    f"haforu process timed out after 30s for text: {text[:50]!r}"
                ) from exc
            except subprocess.CalledProcessError as exc:
                raise RendererInitError(
                    f"haforu process failed (exit {exc.returncode}): {exc.stderr}"
                ) from exc
            except Exception as exc:
                raise RendererInitError(f"haforu subprocess error: {exc}") from exc

            # Parse JSONL output (expect one line per job)
            lines = [line.strip() for line in result.stdout.strip().split("\n") if line.strip()]
            if not lines:
                raise RendererInitError("haforu returned no output")

            # Find our job result
            job_result = None
            for line in lines:
                try:
                    parsed = json.loads(line)
                    if parsed.get("id") == job_id:
                        job_result = parsed
                        break
                except json.JSONDecodeError:
                    continue  # Skip non-JSON lines (e.g., log output)

            if job_result is None:
                raise RendererInitError(f"haforu output missing job {job_id}")

            # Check status
            if job_result.get("status") != "success":
                error_msg = job_result.get("error", "unknown error")
                # Special case: haforu rendering not yet implemented
                if isinstance(error_msg, str) and "not fully implemented" in error_msg.lower():
                    raise RendererUnavailableError(
                        "haforu rendering is not yet implemented. "
                        "The binary exists but rendering functionality is pending. "
                        "Use --renderer=auto or --renderer=coretext/harfbuzz instead."
                    )
                raise RendererInitError(f"haforu job {job_id} failed: {error_msg}")

            # Extract rendering data — support both haforu (file path) and haforu2 (inline base64)
            rendering_result = (
                job_result.get("rendering_result") or job_result.get("rendering") or job_result
            )
            # 1) Inline PGM base64 (preferred in haforu2)
            pgm_b64 = rendering_result.get("pgm_base64")
            if isinstance(pgm_b64, str):
                w = int(rendering_result.get("width") or job_result.get("width") or self.width)
                h = int(rendering_result.get("height") or job_result.get("height") or self.height)
                return _decode_pgm_base64(pgm_b64, w, h)

            # 2) Inline PNG base64 (fallback)
            png_b64 = rendering_result.get("png_base64") or rendering_result.get("image_base64")
            if isinstance(png_b64, str):
                try:
                    import cv2
                except ImportError as exc:
                    raise RendererInitError("cv2 required for PNG base64 image decoding") from exc
                data = base64.b64decode(png_b64)
                arr = np.frombuffer(data, dtype=np.uint8)
                img = cv2.imdecode(arr, flags=0)  # flags=0 => grayscale
                if img is None:
                    raise RendererInitError("Failed to decode inline PNG image from haforu output")
                return img

            # 3) Filesystem identifier (haforu v1)
            image_path = rendering_result.get("identifier")
            if image_path:
                try:
                    import cv2
                except ImportError as exc:
                    raise RendererInitError("cv2 required for haforu image loading") from exc

                img = cv2.imread(str(image_path), cv2.IMREAD_GRAYSCALE)
                if img is None:
                    raise RendererInitError(f"Failed to load rendered image from {image_path}")
                return img

            raise RendererInitError(
                f"haforu job {job_id} missing image data (expected pgm_base64/png_base64 or identifier)"
            )
        finally:
            # Clean up temp directory regardless of success/failure
            try:
                import shutil

                shutil.rmtree(temp_dir, ignore_errors=True)
            except Exception:
                pass
