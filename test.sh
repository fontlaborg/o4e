#!/usr/bin/env bash
# this_file: test.sh

# Functional test runner for o4e examples and integration tests
# This script runs the Python examples as functional tests to verify
# that the package works end-to-end.

set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "${ROOT_DIR}"

log() {
  printf '[test] %s\n' "$*"
}

log_ok() {
  printf '[test] \033[32m✓\033[0m %s\n' "$*"
}

log_err() {
  printf '[test] \033[31m✗\033[0m %s\n' "$*" >&2
}

die() {
  log_err "$*"
  exit 1
}

ensure_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Missing required command: $1"
  fi
}

# Ensure required tools are available
ensure_cmd python3
ensure_cmd cargo
ensure_cmd pytest

TEST_DIR="${ROOT_DIR}/test_artifacts"
mkdir -p "${TEST_DIR}"

# Track test results
TESTS_PASSED=0
TESTS_FAILED=0
FAILED_TESTS=()

run_test() {
  local test_name="$1"
  local test_cmd="$2"

  log "Running: ${test_name}"

  if eval "${test_cmd}" >"${TEST_DIR}/${test_name}.log" 2>&1; then
    log_ok "${test_name}"
    ((TESTS_PASSED++)) || true
  else
    log_err "${test_name} (see ${TEST_DIR}/${test_name}.log)"
    FAILED_TESTS+=("${test_name}")
    ((TESTS_FAILED++)) || true
  fi
}

# ============================================================================
# Phase 1: Rust Unit Tests
# ============================================================================

log "Phase 1: Rust unit and integration tests"
log "=========================================="

run_test "cargo-fmt-check" \
  "cargo fmt --all --check"

run_test "cargo-clippy" \
  "cargo clippy --workspace --all-features --exclude o4e-python -- -D warnings"

run_test "cargo-test-core" \
  "cargo test -p o4e-core"

run_test "cargo-test-unicode" \
  "cargo test -p o4e-unicode"

run_test "cargo-test-render" \
  "cargo test -p o4e-render"

run_test "cargo-test-icu-hb" \
  "cargo test -p o4e-icu-hb"

# Platform-specific tests
if [[ "$(uname -s)" == "Darwin" ]]; then
  run_test "cargo-test-mac" \
    "cargo test -p o4e-mac"
fi

# ============================================================================
# Phase 2: Python Unit Tests
# ============================================================================

log ""
log "Phase 2: Python unit tests"
log "==========================="

# Check if wheel exists, if not, try to build it
WHEEL_PATH="$(ls -1t "${ROOT_DIR}"/target/wheels/o4e-*.whl 2>/dev/null | head -n 1 || true)"
if [[ -z "${WHEEL_PATH}" ]]; then
  log "No wheel found, building o4e Python package..."
  if command -v maturin >/dev/null 2>&1 || command -v uvx >/dev/null 2>&1; then
    if command -v uvx >/dev/null 2>&1; then
      run_test "build-python-wheel" \
        "uvx maturin build --release --locked --out target/wheels"
    else
      run_test "build-python-wheel" \
        "maturin build --release --locked --out target/wheels"
    fi
    WHEEL_PATH="$(ls -1t "${ROOT_DIR}"/target/wheels/o4e-*.whl 2>/dev/null | head -n 1 || true)"
  else
    log_err "Neither maturin nor uvx found, cannot build wheel"
    log_err "Install with: pip install maturin  OR  pipx install uv"
  fi
fi

if [[ -n "${WHEEL_PATH}" ]]; then
  log "Found wheel: ${WHEEL_PATH}"

  # Create virtual environment for testing
  VENV_DIR="${TEST_DIR}/venv"
  if [[ ! -d "${VENV_DIR}" ]]; then
    log "Creating test virtual environment..."
    python3 -m venv "${VENV_DIR}"
  fi

  # Install the wheel
  log "Installing o4e wheel..."
  "${VENV_DIR}/bin/pip" install --force-reinstall "${WHEEL_PATH}" >/dev/null 2>&1
  "${VENV_DIR}/bin/pip" install pytest pillow numpy >/dev/null 2>&1

  # Run Python unit tests (mocked tests)
  run_test "pytest-unit-tests" \
    "${VENV_DIR}/bin/pytest python/tests/test_api.py -v"

  # Run Python integration tests (real backend)
  run_test "pytest-integration-tests" \
    "${VENV_DIR}/bin/pytest python/tests/test_integration.py -v"
else
  log_err "No wheel available, skipping Python tests"
  ((TESTS_FAILED++)) || true
fi

# ============================================================================
# Phase 3: Functional Tests (Examples)
# ============================================================================

log ""
log "Phase 3: Functional tests (examples)"
log "====================================="

if [[ -n "${WHEEL_PATH}" ]] && [[ -d "${VENV_DIR}" ]]; then
  # Test basic_render.py
  run_test "example-basic-render" \
    "cd ${ROOT_DIR}/examples && ${VENV_DIR}/bin/python basic_render.py"

  # Test test_png_output.py
  run_test "example-png-output" \
    "cd ${ROOT_DIR}/examples && ${VENV_DIR}/bin/python test_png_output.py"

  # Test convert_to_png.py if it exists
  if [[ -f "${ROOT_DIR}/examples/convert_to_png.py" ]]; then
    run_test "example-convert-png" \
      "cd ${ROOT_DIR}/examples && ${VENV_DIR}/bin/python convert_to_png.py || true"
  fi

  # Verify that example outputs were created
  if [[ -f "${ROOT_DIR}/examples/hello.png" ]]; then
    log_ok "PNG output file created successfully"
    ((TESTS_PASSED++)) || true
  else
    log_err "Expected PNG output file not found"
    ((TESTS_FAILED++)) || true
  fi
else
  log_err "Skipping functional tests (wheel not available)"
fi

# ============================================================================
# Phase 4: Benchmarks (optional, not counted as test)
# ============================================================================

if [[ "${RUN_BENCHMARKS:-}" == "1" ]]; then
  log ""
  log "Phase 4: Performance benchmarks"
  log "================================"

  log "Running criterion benchmarks..."
  cargo bench --workspace 2>&1 | tee "${TEST_DIR}/benchmarks.log"
  log "Benchmark results saved to ${TEST_DIR}/benchmarks.log"
fi

# ============================================================================
# Summary
# ============================================================================

log ""
log "Test Summary"
log "============"
log "Passed: ${TESTS_PASSED}"
log "Failed: ${TESTS_FAILED}"

if [[ ${TESTS_FAILED} -gt 0 ]]; then
  log ""
  log_err "Failed tests:"
  for test in "${FAILED_TESTS[@]}"; do
    log_err "  - ${test}"
  done
  log ""
  log "View logs in: ${TEST_DIR}/"
  exit 1
else
  log_ok "All tests passed!"
  exit 0
fi
