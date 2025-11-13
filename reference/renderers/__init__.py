# this_file: reference/renderers/__init__.py
"""
Renderer selection utilities and registry.
"""

from __future__ import annotations

import platform
import time
from dataclasses import dataclass
from pathlib import Path
from collections import OrderedDict
from collections.abc import Callable, Iterable, Mapping

import numpy as np

from .base import BaseRenderer, RendererInitError, RendererUnavailableError
from .harfbuzz import HarfBuzzRenderer

ENGINE_BUILDERS: dict[str, Callable[..., BaseRenderer]] = {
    HarfBuzzRenderer.engine: HarfBuzzRenderer,
}

try:
    from .coretext import CoreTextRenderer
except Exception:  # pragma: no cover - optional dependency
    CoreTextRenderer = None  # type: ignore[assignment, misc]
else:
    ENGINE_BUILDERS[CoreTextRenderer.engine] = CoreTextRenderer  # type: ignore[arg-type]

try:
    from .skia import SkiaRenderer
except Exception:  # pragma: no cover - optional dependency
    SkiaRenderer = None  # type: ignore[assignment, misc]
else:
    ENGINE_BUILDERS[SkiaRenderer.engine] = SkiaRenderer  # type: ignore[arg-type]

# Pillow renderer was removed from the project; no import here.

# Haforu Python bindings (fastest path) — optional
try:  # pragma: no cover - availability varies by developer environment
    from .haforu_python import (
        HaforuPythonRenderer,
        shutdown_shared_session as shutdown_haforu_python,
    )
except Exception:
    HaforuPythonRenderer = None  # type: ignore[assignment, misc]
    shutdown_haforu_python = None  # type: ignore[assignment]
else:
    ENGINE_BUILDERS[HaforuPythonRenderer.engine] = HaforuPythonRenderer  # type: ignore[arg-type]

# Haforu (external binary) — initially a stub backend enabling engine selection
try:  # pragma: no cover - availability varies by developer environment
    from .haforu import HaforuRenderer
except Exception:
    HaforuRenderer = None  # type: ignore[assignment, misc]
else:
    ENGINE_BUILDERS[HaforuRenderer.engine] = HaforuRenderer  # type: ignore[arg-type]


def available_engines() -> dict[str, bool]:
    """
    Map engine name to availability status.

    Returns:
        Dictionary mapping engine names to their availability (True/False)
    """
    status: dict[str, bool] = {}
    for name, builder in ENGINE_BUILDERS.items():
        try:
            status[name] = builder.is_available()  # type: ignore[attr-defined]
        except Exception:
            status[name] = False
    return status


def describe_available_engines(status: Mapping[str, bool] | None = None) -> str:
    """
    Render a human-readable list of available engines or ``"none"`` when empty.

    Args:
        status: Optional engine availability mapping (uses available_engines() if None)

    Returns:
        Comma-separated string of available engines, or "none" if no engines available
    """

    engines = status if status is not None else available_engines()
    available = [name for name, ok in engines.items() if ok]
    return ", ".join(available) if available else "none"


def validate_renderer_compatibility(requested_engine: str = "auto") -> tuple[bool, str]:
    """
    Validate that requested renderer engine is available.

    Args:
        requested_engine: Engine name to validate ("auto" or specific engine)

    Returns:
        Tuple of (is_valid, message) where message describes the issue if invalid
    """
    availability = available_engines()
    available_list = [name for name, ok in availability.items() if ok]

    if not available_list:
        return (False, "No renderer engines are available on this system")

    if requested_engine == "auto":
        # Auto mode is always valid if any engine exists
        default = default_engine()
        return (True, f"Auto mode will use: {default}")

    # Check if requested engine is available
    if requested_engine not in ENGINE_BUILDERS:
        return (
            False,
            f"Unknown renderer '{requested_engine}'. Available: {', '.join(available_list)}",
        )

    if not availability.get(requested_engine):
        return (
            False,
            f"Renderer '{requested_engine}' is not available. Available: {', '.join(available_list)}",
        )

    return (True, f"Renderer '{requested_engine}' is available")


def default_engine() -> str:
    """
    Choose the best available renderer engine.

    Preference order reflects performance and integration goals:
    1) haforu-python (PyO3 bindings; fastest, zero IPC)
    2) haforu         (CLI binary; very fast in batch)
    3) native/platform engines (CoreText/Skia/HarfBuzz)

    On macOS, CoreText remains the native fallback when Haforu is unavailable.

    Returns:
        Name of the selected renderer engine (e.g., "haforu-python", "haforu", "coretext")
    """
    availability = available_engines()

    # Prefer Haforu Python bindings when available
    if availability.get("haforu-python"):
        return "haforu-python"

    # Then prefer Haforu CLI engine if available
    if availability.get("haforu"):
        return "haforu"

    # Native/platform engines
    if platform.system() == "Darwin" and availability.get("coretext"):
        return "coretext"
    if availability.get("skia"):
        return "skia"
    if availability.get("harfbuzz"):
        return "harfbuzz"

    # Last resort: any available
    for name, ok in availability.items():
        if ok:
            return name

    raise RendererUnavailableError("No renderer engines are available.")


_POOL_CAPACITY = 64
_RENDERER_POOL: OrderedDict[tuple, BaseRenderer] = OrderedDict()


def _pool_key(
    engine: str,
    font_path: Path,
    *,
    width: int,
    height: int,
    font_size: int,
    features: dict[str, int] | None,
    instance_coords: dict[str, float] | None,
    tracking: float,
) -> tuple:
    features_key: tuple[tuple[str, int], ...] = tuple(sorted((features or {}).items()))
    # IMPORTANT: Do NOT include instance_coords in the pool key.
    # Rationale: During deep matching, coordinates change frequently and
    # we want to reuse a single renderer instance (especially for haforu-python
    # which maintains a persistent streaming session). Coordinates are applied
    # via update_instance_coords() when reusing pooled instances.
    #
    # Tracking remains part of the key to allow co-existence of distinct
    # renderer instances with different letter-spacing in user-facing code.
    return (
        engine,
        str(font_path),
        width,
        height,
        font_size,
        features_key,
        float(tracking),
    )


def clear_renderer_pool() -> None:
    """Clear the global renderer pool (used in tests or shutdown).

    Best-effort close on pooled renderers before clearing to release native
    resources (e.g., Haforu streaming sessions, CoreText/Skia objects).
    """
    try:
        for _, renderer in list(_RENDERER_POOL.items()):
            try:
                close = getattr(renderer, "close", None)
                if callable(close):
                    close()
            except Exception:
                # Ignore renderer-specific shutdown errors
                pass
    finally:
        _RENDERER_POOL.clear()
        if shutdown_haforu_python is not None:
            try:
                shutdown_haforu_python()
            except Exception:
                pass


def get_best_renderer_name(
    prefer: str | None = None,
    *,
    require_haforu: bool = False,
    require_haforu_python: bool = False,
    allow_haforu: bool = True,
) -> str:
    """Return the name of the best available renderer engine.

    This helper wraps ``default_engine()`` with optional preference and
    requirements for Haforu engines to support CLI flags without duplicating
    selection logic elsewhere.

    Args:
        prefer: Optional explicit preference (e.g., 'haforu-python', 'haforu').
        require_haforu: If True, require either haforu-python or haforu to be available.
        require_haforu_python: If True, require haforu-python (PyO3 bindings).
        allow_haforu: When False, skip Haforu engines entirely (CPU debugging mode).

    Returns:
        Engine name string.

    Raises:
        RendererUnavailableError: When requirements cannot be satisfied.
    """
    availability = available_engines()

    if not allow_haforu:
        availability["haforu"] = False
        availability["haforu-python"] = False
        if require_haforu or require_haforu_python:
            raise RendererUnavailableError(
                "Haforu requested but disabled via --cpu-renderer mode"
            )

    if require_haforu_python and not availability.get("haforu-python", False):
        raise RendererUnavailableError("haforu-python required but not available")

    if require_haforu and not (
        availability.get("haforu-python", False) or availability.get("haforu", False)
    ):
        raise RendererUnavailableError("haforu (python or cli) required but not available")

    if prefer and prefer != "auto":
        # Respect explicit preference if available
        if availability.get(prefer, False):
            return prefer
        # If preference isn't available but requirements are met, fall back to default

    return default_engine()


def create_renderer(
    engine: str,
    font_path: Path,
    *,
    instance_coords: dict[str, float] | None = None,
    features: dict[str, int] | None = None,
    width: int,
    height: int,
    font_size: int,
    tracking: float = 0.0,
) -> BaseRenderer:
    """
    Instantiate a renderer engine by name with fallback support.
    """
    engine = engine.lower()
    if engine == "auto":
        engine = default_engine()

    builder = ENGINE_BUILDERS.get(engine)
    if not builder:
        raise RendererInitError(f"Unknown renderer engine '{engine}'")

    if not builder.is_available():  # type: ignore[attr-defined]
        raise RendererUnavailableError(f"Renderer '{engine}' is not available on this system.")

    key = _pool_key(
        engine,
        font_path,
        width=width,
        height=height,
        font_size=font_size,
        features=features or {},
        instance_coords=instance_coords or {},
        tracking=tracking,
    )
    # Try pool
    if key in _RENDERER_POOL:
        renderer = _RENDERER_POOL.pop(key)
        _RENDERER_POOL[key] = renderer  # move to MRU
        try:
            renderer.update_dimensions(width=width, height=height, font_size=font_size)
            renderer.update_tracking(tracking)
            renderer.features = features or {}
            renderer.update_instance_coords(instance_coords or None)
        except Exception:
            pass
        return renderer

    # Build and insert in pool
    try:
        renderer = builder(
            font_path,
            instance_coords=instance_coords,
            features=features,
            width=width,
            height=height,
            font_size=font_size,
            tracking=tracking,
        )
    except TypeError as exc:
        # Backwards compatibility: some tests/registers may provide custom builders
        # that do not accept the optional 'tracking' keyword. Retry without it.
        if "tracking" in str(exc):
            renderer = builder(
                font_path,
                instance_coords=instance_coords,
                features=features,
                width=width,
                height=height,
                font_size=font_size,
            )
        else:
            raise

    _RENDERER_POOL[key] = renderer
    if len(_RENDERER_POOL) > _POOL_CAPACITY:
        # Safely close least-recently-used renderer when evicting from the pool
        try:
            _, evicted = _RENDERER_POOL.popitem(last=False)
        except Exception:
            evicted = None
        if evicted is not None:
            try:
                close = getattr(evicted, "close", None)
                if callable(close):
                    close()
            except Exception:
                # Best-effort cleanup; ignore renderer-specific close errors
                pass
    return renderer


def create_renderer_with_fallback(
    engine: str,
    font_path: Path,
    *,
    instance_coords: dict[str, float] | None = None,
    features: dict[str, int] | None = None,
    width: int,
    height: int,
    font_size: int,
    tracking: float = 0.0,
) -> BaseRenderer | None:
    """
    Try to create a renderer, falling back to alternatives if the requested one fails.

    Returns None if no renderer can be created.
    """
    from loguru import logger

    # Try the requested engine first
    if engine != "auto":
        try:
            return create_renderer(
                engine,
                font_path,
                instance_coords=instance_coords,
                features=features,
                width=width,
                height=height,
                font_size=font_size,
                tracking=tracking,
            )
        except (RendererInitError, RendererUnavailableError) as exc:
            logger.warning(f"Renderer {engine} failed: {exc}, trying alternatives")

    # Try all available engines
    availability = available_engines()

    # On macOS, prefer CoreText first (native, stable, no segfaults with variable fonts)
    if platform.system() == "Darwin":
        preferred_order = ["coretext", "skia", "harfbuzz"]
    else:
        preferred_order = ["skia", "harfbuzz", "coretext"]

    for eng in preferred_order:
        if availability.get(eng):
            try:
                return create_renderer(
                    eng,
                    font_path,
                    instance_coords=instance_coords,
                    features=features,
                    width=width,
                    height=height,
                    font_size=font_size,
                    tracking=tracking,
                )
            except Exception as exc:
                logger.debug(f"Fallback renderer {eng} failed: {exc}")
                continue

    # Try any other available engine
    for eng, available in availability.items():
        if available and eng not in preferred_order:
            try:
                logger.debug(f"Trying last resort renderer: {eng}")
                return create_renderer(
                    eng,
                    font_path,
                    instance_coords=instance_coords,
                    features=features,
                    width=width,
                    height=height,
                    font_size=font_size,
                    tracking=tracking,
                )
            except Exception as exc:
                logger.debug(f"Last resort renderer {eng} failed: {exc}")
                continue

    logger.error("No renderer could be initialized for font: %s", font_path.name)
    return None


@dataclass(slots=True)
class BenchmarkResult:
    """
    Results from benchmarking a renderer engine.

    Attributes:
        engine: Name of the renderer engine that was benchmarked
        iterations: Number of rendering iterations performed
        elapsed: Total elapsed time in seconds for all iterations
    """

    engine: str
    iterations: int
    elapsed: float

    @property
    def per_iteration_ms(self) -> float:
        """Calculate average time per iteration in milliseconds."""
        return (self.elapsed / self.iterations) * 1000


def benchmark_engines(
    text: str,
    font_path: Path,
    *,
    instance_coords: Mapping[str, float] | None = None,
    features: Mapping[str, int] | None = None,
    engines: Iterable[str] | None = None,
    iterations: int = 5,
    width: int = 1200,
    height: int = 200,
    font_size: int = 72,
) -> list[BenchmarkResult]:
    """
    Benchmark available renderer engines and return timings.
    """
    if iterations <= 0:
        raise ValueError("iterations must be positive")

    selected = (
        list(engines)
        if engines
        else [name for name, available in available_engines().items() if available]
    )
    results: list[BenchmarkResult] = []

    for engine_name in selected:
        try:
            renderer = create_renderer(
                engine_name,
                font_path,
                instance_coords=dict(instance_coords or {}),
                features=dict(features or {}),
                width=width,
                height=height,
                font_size=font_size,
            )
        except RendererUnavailableError:
            continue

        start = time.perf_counter()
        for _ in range(iterations):
            output = renderer.render_text(text)
            if not isinstance(output, np.ndarray):
                raise RendererInitError(f"Renderer {engine_name} returned unexpected result type")
        elapsed = time.perf_counter() - start
        results.append(BenchmarkResult(engine=engine_name, iterations=iterations, elapsed=elapsed))

    results.sort(key=lambda r: r.per_iteration_ms)
    return results
