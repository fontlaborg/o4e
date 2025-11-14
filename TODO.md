---
this_file: TODO.md
---

# TODO

## Code Simplification
- [x] Remove fuzz/ directory
- [x] Remove .github/workflows/fuzz.yml
- [x] Remove backends/o4e-core/src/diagnostics.rs
- [x] Remove benches/ directory
- [x] Remove RenderOptionsDiagnostics from all backend imports and usage
- [x] Remove benchmark configuration from Cargo.toml

## Testing & Verification
- [x] Run cargo test --workspace
- [x] Run pytest python/tests -v
- [x] Verify all 13 tests still pass
- [ ] Check memory usage (< 50MB target)
- [ ] Measure render times (< 1ms target)

## Documentation Cleanup
- [ ] Review README.md (keep under 150 lines)
- [ ] Remove verbose comments from code
- [ ] Update examples/basic_render.py to be minimal
- [ ] Remove examples/convert_to_png.py if not needed

## Packaging
- [ ] Test wheel build on macOS
- [ ] Test wheel build on Windows
- [ ] Test wheel build on Linux
- [ ] Verify pip install works
- [ ] Check dependency list is minimal

## Release Preparation
- [ ] Update CHANGELOG.md with v0.1.0 notes
- [ ] Create git tag v0.1.0
- [ ] Build release wheels
- [ ] Test installation from wheel
