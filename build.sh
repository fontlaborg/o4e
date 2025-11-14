#!/usr/bin/env bash
# this_file: build.sh

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

log() {
  printf '[build] %s\n' "$*"
}

die() {
  printf '[build][error] %s\n' "$*" >&2
  exit 1
}

ensure_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Missing required command: $1"
  fi
}

ensure_cmd cargo
ensure_cmd uvx

WHEEL_DIR="${ROOT_DIR}/target/wheels"
REF_HAFORU_DIR="${ROOT_DIR}/reference/haforu"
REF_WHEEL_DIR="${REF_HAFORU_DIR}/target/wheels"

log "Running cargo fmt"
cargo fmt --all

log "Running cargo clippy (workspace, skip benches/python bindings)"
cargo clippy --workspace --all-features --locked --exclude o4e-python

log "Running cargo test (workspace)"
cargo test --workspace --all-features --locked --exclude o4e-python

log "Building Rust workspace release artifacts"
cargo build --workspace --release --all-features --locked --exclude o4e-python

log "Running Python tests via hatch"
if uvx hatch test; then
  :
else
  status=$?
  if [[ ${status} -eq 5 ]]; then
    log "pytest returned exit code 5 (no tests collected); continuing"
  else
    die "uvx hatch test failed (exit ${status})"
  fi
fi

log "Building Python wheel (o4e)"
mkdir -p "${WHEEL_DIR}"
uvx maturin build --release --locked --out "${WHEEL_DIR}"

log "Building reference haforu CLI (release)"
cargo build --manifest-path "${REF_HAFORU_DIR}/Cargo.toml" --release --locked

if [[ -f "${REF_HAFORU_DIR}/pyproject.toml" && -f "${REF_HAFORU_DIR}/README.md" ]]; then
  log "Building reference haforu Python wheel"
  mkdir -p "${REF_WHEEL_DIR}"
  (
    cd "${REF_HAFORU_DIR}"
    uvx maturin build --release --locked --out "${REF_WHEEL_DIR}"
  )
else
  log "Skipping reference haforu Python wheel (pyproject README missing)"
fi

log "Build complete. Rust artifacts in target/release, wheels in target/wheels and reference/haforu/target/wheels."
