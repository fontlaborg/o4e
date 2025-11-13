# this_file: reference/renderers/haforu_batch.py
"""Haforu CLI batch rendering helper."""

from __future__ import annotations

import json
import os
import subprocess
from collections import defaultdict
from concurrent.futures import ThreadPoolExecutor, as_completed
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable

import numpy as np

from .haforu import _decode_pgm_base64, _resolve_haforu_bin
from .base import RendererInitError, RendererUnavailableError

HAFORU_BATCH_SIZE = 2048
ENCODER_WORKERS = max(2, min(4, (os.cpu_count() or 4) // 2))


@dataclass(frozen=True)
class RenderRequest:
    """Single Haforu render request."""

    job_id: str
    text: str
    width: int
    height: int
    font_size: int
    script: str = "Latn"
    direction: str = "ltr"
    language: str = "en"


class HaforuBatchRunner:
    """Manage batched Haforu CLI invocations."""

    def __init__(
        self,
        font_path: Path,
        variations: dict[str, float] | None,
        *,
        cache_size: int = 512,
        haforu_bin: Path | None = None,
    ) -> None:
        self.font_path = Path(font_path)
        self.variations = dict(variations or {})
        self.cache_size = cache_size
        self.bin_path = haforu_bin or _resolve_haforu_bin()
        if self.bin_path is None:
            raise RendererUnavailableError("haforu binary not available for batch rendering")

    def render_requests(self, requests: Iterable[RenderRequest]) -> dict[str, np.ndarray]:
        req_list = list(requests)
        if not req_list:
            return {}

        request_map = {req.job_id: req for req in req_list}
        chunks: list[list[RenderRequest]] = []
        for idx in range(0, len(req_list), HAFORU_BATCH_SIZE):
            chunks.append(req_list[idx : idx + HAFORU_BATCH_SIZE])

        results: dict[str, np.ndarray] = {}
        with ThreadPoolExecutor(max_workers=min(len(chunks), ENCODER_WORKERS)) as executor:
            future_map = {
                executor.submit(self._encode_chunk, chunk): chunk for chunk in chunks
            }
            for future in as_completed(future_map):
                encoded_chunk = future.result()
                response_lines = self._invoke_chunk(encoded_chunk)
                for line in response_lines:
                    if not line.strip():
                        continue
                    payload = json.loads(line)
                    job_id = payload.get("id")
                    if not job_id:
                        raise RendererInitError("haforu response missing job id")
                    if payload.get("status") != "success":
                        error_msg = payload.get("error", "unknown error")
                        raise RendererInitError(f"haforu job {job_id} failed: {error_msg}")
                    rendering = payload.get("rendering") or {}
                    data = rendering.get("data")
                    width = int(rendering.get("width", 0))
                    height = int(rendering.get("height", 0))
                    if data is None:
                        raise RendererInitError(f"haforu job {job_id} missing image data")
                    results[job_id] = _decode_pgm_base64(data, width, height)

        # Ensure every request resolved
        missing = [job_id for job_id in request_map if job_id not in results]
        if missing:
            raise RendererInitError(f"Missing haforu results for jobs: {', '.join(missing)}")

        return results

    def _encode_chunk(self, chunk: list[RenderRequest]) -> list[str]:
        lines: list[str] = []
        for req in chunk:
            job = {
                "id": req.job_id,
                "font": {
                    "path": str(self.font_path),
                    "size": req.font_size,
                    "variations": self.variations,
                },
                "text": {
                    "content": req.text,
                    "script": req.script,
                },
                "rendering": {
                    "format": "pgm",
                    "encoding": "base64",
                    "width": req.width,
                    "height": req.height,
                },
            }
            lines.append(json.dumps(job, separators=(",", ":")))
        return lines

    def _invoke_chunk(self, encoded_chunk: list[str]) -> list[str]:
        payload = "\n".join(encoded_chunk)
        cmd = [str(self.bin_path), "batch", f"--cache-size={self.cache_size}"]
        proc = subprocess.run(
            cmd,
            input=payload.encode("utf-8"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=120,
        )
        if proc.returncode != 0:
            stderr = proc.stderr.decode("utf-8", errors="replace")
            raise RendererInitError(
                f"haforu batch exited with {proc.returncode}: {stderr.strip()}"
            )
        stdout = proc.stdout.decode("utf-8", errors="replace")
        return [line for line in stdout.splitlines() if line.strip()]


def build_cached_renderers(
    runner: HaforuBatchRunner,
    requests: list[RenderRequest],
) -> dict[tuple[int, int, int], dict[str, np.ndarray]]:
    """Render requests and group them by (width, height, font_size)."""

    cache_map: dict[tuple[int, int, int], dict[str, np.ndarray]] = defaultdict(dict)
    images = runner.render_requests(requests)
    lookup = {req.job_id: req for req in requests}
    for job_id, image in images.items():
        req = lookup.get(job_id)
        if req is None:
            continue
        key = (req.width, req.height, req.font_size)
        cache_map[key][req.text] = image
    return cache_map
