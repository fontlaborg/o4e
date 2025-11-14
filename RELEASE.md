# o4e v0.1.0 Release Checklist

## Development Status: ✅ COMPLETE

All 28 development tasks have been completed. The project is ready for v0.1.0 release.

## Pre-Release Checklist

### 1. Testing & Verification

- [ ] **Run local test suite**
  ```bash
  ./test.sh
  ```
  - Verify all Rust tests pass
  - Verify all Python tests pass
  - Verify example scripts run successfully

- [ ] **Verify builds on all platforms**
  ```bash
  # macOS
  cargo build --workspace --release --exclude o4e-python

  # Linux (via CI or local)
  cargo build --workspace --release --exclude o4e-python --features icu

  # Windows (via CI)
  # Verify via GitHub Actions
  ```

- [ ] **Build Python wheels**
  ```bash
  # macOS
  maturin build --release --features python,mac,icu

  # Verify wheel contents
  unzip -l target/wheels/o4e-*.whl
  ```

- [ ] **Test Python package locally**
  ```bash
  pip install target/wheels/o4e-*.whl
  python -c "import o4e; print(o4e.__version__)"
  python examples/basic_render.py
  ```

### 2. Documentation Review

- [ ] **Verify README.md**
  - Installation instructions accurate
  - Quick start examples work
  - Links to documentation valid
  - Line count < 220 ✅ (currently 214)

- [ ] **Review CHANGELOG.md**
  - All changes for v0.1.0 documented
  - Format follows Keep a Changelog
  - No unreleased changes remain

- [ ] **Check docs/backends.md**
  - Backend comparison table accurate
  - Usage examples correct
  - Troubleshooting section complete

### 3. Version Updates

- [ ] **Update version in Cargo.toml**
  ```toml
  [workspace.package]
  version = "0.1.0"
  ```

- [ ] **Update version in pyproject.toml**
  ```toml
  [project]
  version = "0.1.0"
  ```

- [ ] **Update CHANGELOG.md**
  - Change `## [Unreleased]` to `## [0.1.0] - 2024-11-14`
  - Add link at bottom: `[0.1.0]: https://github.com/fontlaborg/o4e/releases/tag/v0.1.0`

### 4. Git Preparation

- [ ] **Commit all changes**
  ```bash
  git status  # Verify clean working directory
  git add -A
  git commit -m "chore: prepare v0.1.0 release"
  ```

- [ ] **Create and push tag**
  ```bash
  git tag -a v0.1.0 -m "Release v0.1.0: Tri-backend MVP"
  git push origin main
  git push origin v0.1.0
  ```

### 5. Release Workflow

- [ ] **Monitor GitHub Actions**
  - Watch CI workflow pass on tag
  - Watch release workflow build wheels
  - Verify artifacts are created

- [ ] **Verify Artifacts**
  - Python wheels for all platforms (macOS, Windows, Linux)
  - Python wheels for all versions (3.8, 3.9, 3.10, 3.11, 3.12)
  - Source distribution (.tar.gz)

### 6. Publishing

#### PyPI Release

- [ ] **Test on TestPyPI first** (optional but recommended)
  ```bash
  # Upload to TestPyPI
  maturin upload --repository testpypi target/wheels/*

  # Test installation
  pip install --index-url https://test.pypi.org/simple/ o4e
  ```

- [ ] **Publish to PyPI**
  - GitHub Actions will automatically publish when the tag is pushed
  - OR manually: `maturin upload target/wheels/*`

- [ ] **Verify PyPI listing**
  - Visit https://pypi.org/project/o4e/
  - Check project description renders correctly
  - Verify all wheels are available
  - Test installation: `pip install o4e`

#### Crates.io Release

- [ ] **Publish crates in order**
  ```bash
  # 1. Core types
  cd backends/o4e-core && cargo publish
  sleep 30

  # 2. Shared utilities
  cd ../../crates/o4e-fontdb && cargo publish
  sleep 30
  cd ../o4e-unicode && cargo publish
  sleep 30
  cd ../o4e-render && cargo publish
  sleep 30

  # 3. Backends
  cd ../../backends/o4e-icu-hb && cargo publish
  sleep 30
  cd ../o4e-mac && cargo publish  # macOS only
  sleep 30
  cd ../o4e-win && cargo publish  # Windows only
  sleep 30

  # 4. Main crate
  cd ../.. && cargo publish
  ```

- [ ] **Verify crates.io listings**
  - Check https://crates.io/crates/o4e
  - Verify documentation builds on docs.rs
  - Test installation: `cargo install o4e`

### 7. GitHub Release

- [ ] **Create GitHub Release**
  - Go to https://github.com/fontlaborg/o4e/releases/new
  - Use tag: `v0.1.0`
  - Release title: `v0.1.0 - Tri-Backend MVP`
  - Description: Copy from CHANGELOG.md with highlights
  - Attach artifacts:
    - Source code (auto-attached)
    - Sample rendered outputs (create `artifacts/` directory)

- [ ] **Update release notes**
  ```markdown
  # o4e v0.1.0 - Tri-Backend MVP

  First stable release of the Open Font Engine!

  ## Features

  - ✅ Three backends: CoreText (macOS), DirectWrite (Windows), ICU+HarfBuzz (cross-platform)
  - ✅ Complete Python API with type hints
  - ✅ Multiple output formats: PNG, SVG, raw RGBA
  - ✅ Comprehensive Unicode support
  - ✅ High-performance batch rendering
  - ✅ Security fuzzing infrastructure

  ## Installation

  ```bash
  pip install o4e
  ```

  ## Quick Start

  ```python
  from o4e import TextRenderer, Font

  renderer = TextRenderer()
  image = renderer.render(
      text="Hello, World!",
      font=Font("Arial", size=48),
      format="png"
  )
  ```

  See [README.md](README.md) for full documentation.
  ```

### 8. Post-Release Tasks

- [ ] **Announce release**
  - Update project README if needed
  - Post on relevant forums/communities
  - Tweet/social media announcement

- [ ] **Monitor for issues**
  - Watch GitHub Issues for bug reports
  - Monitor PyPI download stats
  - Check fuzzing workflow results

- [ ] **Archive sample renders**
  ```bash
  mkdir -p artifacts/v0.1.0
  # Run examples and save outputs
  python examples/basic_render.py
  # Save outputs to artifacts/
  ```

- [ ] **Update documentation site** (if applicable)
  - Update version in docs
  - Publish new documentation

## Rollback Plan

If issues are discovered after release:

1. **Yank from PyPI** (doesn't delete, just hides)
   ```bash
   pip install twine
   twine upload --skip-existing --repository-url https://upload.pypi.org/legacy/ \
     --username __token__ --password $PYPI_TOKEN
   ```

2. **Yank from crates.io**
   ```bash
   cargo yank --vers 0.1.0 o4e
   ```

3. **Fix issues and release v0.1.1**

## Success Criteria

Release is considered successful when:

- [x] All development tasks complete (28/28)
- [ ] All tests pass on all platforms
- [ ] Wheels build successfully for all targets
- [ ] PyPI package installs and imports correctly
- [ ] Crates.io documentation builds without errors
- [ ] Examples run successfully
- [ ] No critical bugs reported within 48 hours

## Notes

- Default GitHub Actions should handle most publishing automatically
- Keep PYPI_API_TOKEN and CRATES_IO_TOKEN secrets up to date
- Fuzzing will continue running nightly after release

---

**Last Updated**: 2024-11-14
**Status**: Development Complete, Ready for Release
