---
this_file: testdata/fonts/README.md
---

# Test Fonts

This directory hosts open-source fonts used exclusively for automated tests.

## Noto Sans Regular
- Source: https://github.com/googlefonts/noto-fonts
- License: SIL Open Font License 1.1
- Purpose: Deterministic shaping/rendering tests in the HarfBuzz backend and SVG renderer comparisons.

The font file `NotoSans-Regular.ttf` remains unmodified and is redistributed under the terms of the SIL OFL 1.1. The license is compatible with the repository's licensing, and the font is only used in non-production test scenarios.

## Noto Naskh Arabic Regular
- Source: https://github.com/googlefonts/noto-fonts
- License: SIL Open Font License 1.1
- Purpose: Arabic ligature and contextual shaping regression tests in the ICU+HarfBuzz backend.

## Noto Sans Devanagari Regular
- Source: https://github.com/googlefonts/noto-fonts
- License: SIL Open Font License 1.1
- Purpose: Indic reordering (matra placement, conjuncts) regression tests in the ICU+HarfBuzz backend.

## Noto Sans CJK SC Regular
- Source: https://github.com/googlefonts/noto-cjk
- License: SIL Open Font License 1.1
- Purpose: Deterministic CJK snapshot tests for the SVG renderer.

## Noto Color Emoji COLRv1
- Source: https://github.com/googlefonts/color-fonts
- License: Apache License 2.0
- Purpose: Validating COLRv1/CPAL color glyph rendering and gradient emission in SVG snapshots.
