#!/usr/bin/env bash
# this_file: run.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

log() {
  printf '[run] %s\n' "$*"
}

die() {
  printf '[run][error] %s\n' "$*" >&2
  exit 1
}

ensure_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Missing required command: $1"
  fi
}

ensure_cmd cargo
ensure_cmd uv
ensure_cmd python3

DATA_DIR="${ROOT_DIR}/testdata/fonts"
ARTIFACT_DIR="${ROOT_DIR}/run_artifacts"
mkdir -p "${ARTIFACT_DIR}"

FONT_LATIN="${DATA_DIR}/NotoSans-Regular.ttf"
FONT_ARABIC="${DATA_DIR}/NotoNaskhArabic-Regular.ttf"
FONT_DEVANAGARI="${DATA_DIR}/NotoSansDevanagari-Regular.ttf"

case "$(uname -s)" in
  Darwin) PY_FONT_FAMILY="Helvetica" ;;
  *) PY_FONT_FAMILY="DejaVu Sans" ;;
esac

for font in "${FONT_LATIN}" "${FONT_ARABIC}" "${FONT_DEVANAGARI}"; do
  if [[ ! -f "${font}" ]]; then
    die "Missing font fixture: ${font}"
  fi
done

jobs_file="${ARTIFACT_DIR}/haforu_jobs.jsonl"
cat >"${jobs_file}" <<JSONL
{"id":"latin","font":{"path":"${FONT_LATIN}","size":96,"variations":{}},"text":{"content":"Hello, o4e!"},"rendering":{"format":"png","encoding":"base64","width":512,"height":256}}
{"id":"arabic","font":{"path":"${FONT_ARABIC}","size":96,"variations":{}},"text":{"content":"\u0645\u0631\u062d\u0628\u0627"},"rendering":{"format":"png","encoding":"base64","width":512,"height":256}}
{"id":"devanagari","font":{"path":"${FONT_DEVANAGARI}","size":96,"variations":{}},"text":{"content":"\u0928\u092e\u0938\u094d\u0924\u0947"},"rendering":{"format":"png","encoding":"base64","width":512,"height":256}}
JSONL

haforu_output="${ARTIFACT_DIR}/haforu-output.jsonl"
log "Running haforu CLI against sample jobs"
cargo run --manifest-path "${ROOT_DIR}/reference/haforu/Cargo.toml" --release -- stream \
  --cache-size 64 \
  --base-dir "${DATA_DIR}" \
  <"${jobs_file}" \
  >"${haforu_output}"

log "Decoding haforu render outputs → PNG files"
RUN_ARTIFACT_DIR="${ARTIFACT_DIR}" RUN_HAFORU_OUTPUT="${haforu_output}" python3 <<'PYCODE'
import base64
import json
import os
from pathlib import Path

out_dir = Path(os.environ["RUN_ARTIFACT_DIR"])
out_dir.mkdir(parents=True, exist_ok=True)
source = Path(os.environ["RUN_HAFORU_OUTPUT"])
assets = 0

with source.open("r", encoding="utf-8") as handle:
    for line in handle:
        line = line.strip()
        if not line:
            continue
        record = json.loads(line)
        job_id = record.get("id", "unknown")
        status = record.get("status")
        if status != "success":
            print(f"[run][warn] Job {job_id} failed: {record.get('error')}")
            continue
        rendering = record.get("rendering") or {}
        data = rendering.get("data")
        fmt = rendering.get("format", "png")
        if not data:
            print(f"[run][warn] Job {job_id} missing output data")
            continue
        target = out_dir / f"haforu_{job_id}.{fmt}"
        target.write_bytes(base64.b64decode(data))
        print(f"[run] Wrote {target}")
        assets += 1

if assets == 0:
    raise SystemExit("No haforu outputs were decoded")
PYCODE

wheel_path="$(ls -1t "${ROOT_DIR}"/target/wheels/o4e-*.whl 2>/dev/null | head -n 1 || true)"
if [[ -z "${wheel_path}" ]]; then
  die "No o4e wheel found in target/wheels. Run ./build.sh first."
fi

PY_ENV="$(mktemp -d "${ARTIFACT_DIR}/pyenv.XXXXXX")"
cleanup_env() {
  rm -rf "${PY_ENV}"
}
trap cleanup_env EXIT

log "Creating isolated Python environment for demo"
uv venv --python 3.12 "${PY_ENV}" >/dev/null
log "Installing o4e wheel into demo environment"
uv pip install --python "${PY_ENV}/bin/python" "${wheel_path}" >/dev/null

python_demo_png="${ARTIFACT_DIR}/python_demo.png"
log "Rendering text via Python bindings → ${python_demo_png}"
"${PY_ENV}/bin/python" <<PYTHON
from pathlib import Path
import sys
import o4e

output_path = Path(r"${python_demo_png}")
renderer = o4e.TextRenderer(backend="harfbuzz")
font = o4e.Font("${PY_FONT_FAMILY}", size=64, weight=500)
result = renderer.render("Run script demo", font, format="png")

if isinstance(result, tuple):
    payload = result[0]
else:
    payload = result

if not isinstance(payload, (bytes, bytearray, memoryview)):
    payload = bytes(payload)

output_path.write_bytes(bytes(payload))
print(f"[run] Saved {output_path}")
PYTHON

log "Demo complete. Inspect artifacts under ${ARTIFACT_DIR}"
