// this_file: crates/o4e-render/src/svg.rs

//! SVG rendering implementation for o4e.

use o4e_core::{types::BoundingBox, Glyph, ShapingResult, SvgOptions};
use std::fmt::Write;

/// SVG renderer for converting shaped text to SVG format.
pub struct SvgRenderer {
    precision: usize,
    simplify: bool,
}

impl Default for SvgRenderer {
    fn default() -> Self {
        Self {
            precision: 2,
            simplify: true,
        }
    }
}

impl SvgRenderer {
    /// Create a new SVG renderer with options.
    pub fn new(options: &SvgOptions) -> Self {
        Self {
            precision: options.precision as usize,
            simplify: options.simplify,
        }
    }

    /// Render shaped text to SVG string.
    pub fn render(&self, shaped: &ShapingResult, options: &SvgOptions) -> String {
        let mut svg = String::with_capacity(1024);

        // Calculate bounding box
        let bbox = calculate_svg_bbox(&shaped.glyphs, shaped.bbox.clone());

        // Write SVG header
        let _ = write!(
            &mut svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{:.p$} {:.p$} {:.p$} {:.p$}">"#,
            bbox.x,
            bbox.y,
            bbox.width,
            bbox.height,
            p = self.precision
        );

        svg.push('\n');

        // Start a group for the text
        svg.push_str(r#"  <g id="text">"#);
        svg.push('\n');

        // Render each glyph as a path (placeholder for now)
        for (i, glyph) in shaped.glyphs.iter().enumerate() {
            if options.include_paths {
                let path = extract_glyph_path(glyph, shaped.font.as_ref());
                let simplified = if self.simplify && !path.is_empty() {
                    simplify_path(&path, self.precision)
                } else {
                    path
                };

                if !simplified.is_empty() {
                    let _ = write!(
                        &mut svg,
                        r#"    <path id="glyph-{}" d="{}" transform="translate({:.p$}, {:.p$})" />"#,
                        i,
                        simplified,
                        glyph.x,
                        glyph.y,
                        p = self.precision
                    );
                    svg.push('\n');
                }
            } else {
                // Simple rectangle placeholder when path extraction is not enabled
                let _ = write!(
                    &mut svg,
                    r#"    <rect x="{:.p$}" y="{:.p$}" width="{:.p$}" height="1" />"#,
                    glyph.x,
                    glyph.y - 0.5,
                    glyph.advance,
                    p = self.precision
                );
                svg.push('\n');
            }
        }

        // Close group
        svg.push_str("  </g>\n");

        // Close SVG
        svg.push_str("</svg>");

        svg
    }

    /// Render a single glyph to SVG path string.
    pub fn render_glyph(&self, glyph: &Glyph) -> String {
        extract_glyph_path(glyph, None)
    }
}

/// Calculate SVG bounding box from glyphs.
fn calculate_svg_bbox(glyphs: &[Glyph], fallback: BoundingBox) -> BoundingBox {
    if glyphs.is_empty() {
        return fallback;
    }

    // For SVG, we need to include the full advance width
    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for glyph in glyphs {
        min_x = min_x.min(glyph.x);
        max_x = max_x.max(glyph.x + glyph.advance);

        // Estimate glyph height (this is a simplification)
        min_y = min_y.min(glyph.y - 1.0);
        max_y = max_y.max(glyph.y + 0.5);
    }

    BoundingBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// Extract SVG path from a glyph.
fn extract_glyph_path(_glyph: &Glyph, _font: Option<&o4e_core::Font>) -> String {
    // This would require access to the font data and glyph outlines
    // For now, return a placeholder path
    // In a real implementation, this would:
    // 1. Load the font face
    // 2. Get the glyph outline
    // 3. Convert to SVG path commands

    // Placeholder: simple rectangle path
    String::new()
}

/// Simplify an SVG path using Douglas-Peucker algorithm.
fn simplify_path(path: &str, precision: usize) -> String {
    // For now, just return the path with rounded coordinates
    // A real implementation would use the ramer-douglas-peucker crate

    if path.is_empty() {
        return String::new();
    }

    // Simple coordinate rounding
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch.is_ascii_digit() || ch == '.' || ch == '-' {
            // Start of a number
            let mut num = String::new();
            num.push(ch);

            while let Some(&next_ch) = chars.peek() {
                if next_ch.is_ascii_digit() || next_ch == '.' || next_ch == '-' {
                    num.push(chars.next().unwrap());
                } else {
                    break;
                }
            }

            // Parse and round the number
            if let Ok(val) = num.parse::<f32>() {
                let factor = 10_f32.powi(precision as i32);
                let rounded = (val * factor).round() / factor;
                let _ = write!(&mut result, "{:.p$}", rounded, p = precision);
            } else {
                result.push_str(&num);
            }
        } else {
            result.push(ch);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_renderer_creation() {
        let renderer = SvgRenderer::default();
        assert_eq!(renderer.precision, 2);
        assert!(renderer.simplify);
    }

    #[test]
    fn test_empty_render() {
        let renderer = SvgRenderer::default();
        let shaped = ShapingResult {
            glyphs: vec![],
            advance: 0.0,
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            font: None,
        };

        let svg = renderer.render(&shaped, &SvgOptions::default());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_path_simplification() {
        let path = "M 10.123456 20.987654 L 30.111111 40.999999";
        let simplified = simplify_path(path, 2);
        assert!(simplified.contains("10.12"));
        assert!(simplified.contains("20.99"));
    }
}
