// this_file: crates/o4e-render/src/svg.rs

//! SVG rendering implementation for o4e.

use crate::outlines::glyph_bez_path as recorded_glyph_path;
use kurbo::{BezPath, PathEl, Point};
use log::debug;
use o4e_core::{types::BoundingBox, Font, Glyph, ShapingResult, SvgOptions};
use o4e_fontdb::FontDatabase;
use owned_ttf_parser::{AsFaceRef, OwnedFace};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Write;
use std::sync::{Arc, OnceLock};
use thiserror::Error;
use ttf_parser::{FaceParsingError, GlyphId};

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
            precision: options.precision,
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
            let mut emitted_path = false;
            if options.include_paths {
                if let Some(path_data) = self.glyph_path_data(glyph, shaped.font.as_ref()) {
                    let _ = write!(
                        &mut svg,
                        r#"    <path id="glyph-{}" d="{}" transform="translate({:.p$}, {:.p$})" />"#,
                        i,
                        path_data,
                        glyph.x,
                        glyph.y,
                        p = self.precision
                    );
                    svg.push('\n');
                    emitted_path = true;
                }
            }

            if !emitted_path {
                // Simple rectangle placeholder when path extraction is not available
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

    /// Render a single glyph to SVG path string without font context (best effort).
    pub fn render_glyph(&self, glyph: &Glyph) -> String {
        self.glyph_path_data(glyph, None).unwrap_or_default()
    }

    /// Render a single glyph when the font is known, returning SVG path data if available.
    pub fn render_glyph_with_font(&self, glyph: &Glyph, font: &Font) -> Option<String> {
        self.glyph_path_data(glyph, Some(font))
    }

    fn glyph_path_data(&self, glyph: &Glyph, font: Option<&Font>) -> Option<String> {
        let outline = svg_outline(font, glyph)?;
        let processed = if self.simplify {
            simplify_path(outline, self.precision)
        } else {
            outline
        };

        if processed.elements().is_empty() {
            return None;
        }

        Some(path_to_string(&processed, self.precision))
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

fn svg_outline(font: Option<&Font>, glyph: &Glyph) -> Option<BezPath> {
    let font = font?;
    let glyph_id = GlyphId(u16::try_from(glyph.id).ok()?);
    let (face, scale) = face_and_scale(font)?;
    recorded_glyph_path(face.as_face_ref(), glyph_id, scale)
}

fn face_and_scale(font: &Font) -> Option<(Arc<OwnedFace>, f32)> {
    if font.size <= 0.0 {
        return None;
    }

    let face = font_store().face_for(font).map_err(|err| err).ok()?;

    let units = face.as_face_ref().units_per_em();
    if units == 0 {
        return None;
    }

    Some((face, font.size / units as f32))
}

fn simplify_path(path: BezPath, precision: usize) -> BezPath {
    if path.elements().len() <= 2 {
        return path;
    }

    let tolerance = (10f64.powi(-(precision as i32).max(0)) * 0.1).max(1e-6);
    let tol_sq = tolerance * tolerance;
    let mut simplified = BezPath::new();
    let mut last_point: Option<Point> = None;

    for element in path.elements().iter().copied() {
        match element {
            PathEl::MoveTo(p) => {
                simplified.move_to(p);
                last_point = Some(p);
            }
            PathEl::LineTo(p) => {
                let keep = last_point
                    .map(|prev| squared_distance(prev, p) >= tol_sq)
                    .unwrap_or(true);
                if keep {
                    simplified.line_to(p);
                    last_point = Some(p);
                }
            }
            PathEl::QuadTo(_, p) => {
                simplified.push(element);
                last_point = Some(p);
            }
            PathEl::CurveTo(_, _, p) => {
                simplified.push(element);
                last_point = Some(p);
            }
            PathEl::ClosePath => {
                simplified.close_path();
                last_point = None;
            }
        }
    }

    simplified
}

fn path_to_string(path: &BezPath, precision: usize) -> String {
    if path.elements().is_empty() {
        return String::new();
    }

    let mut data = String::with_capacity(path.elements().len() * 16);
    let mut first = true;

    for &el in path.elements() {
        if !first {
            data.push(' ');
        }
        first = false;
        match el {
            PathEl::MoveTo(p) => append_command(&mut data, 'M', &[p], precision),
            PathEl::LineTo(p) => append_command(&mut data, 'L', &[p], precision),
            PathEl::QuadTo(p1, p2) => append_command(&mut data, 'Q', &[p1, p2], precision),
            PathEl::CurveTo(p1, p2, p3) => append_command(&mut data, 'C', &[p1, p2, p3], precision),
            PathEl::ClosePath => data.push('Z'),
        }
    }

    data
}

fn append_command(buf: &mut String, cmd: char, points: &[Point], precision: usize) {
    buf.push(cmd);
    let mut iter = points.iter();
    if let Some(first_point) = iter.next() {
        append_point(buf, *first_point, precision);
        for point in iter {
            buf.push(' ');
            append_point(buf, *point, precision);
        }
    }
}

fn append_point(buf: &mut String, point: Point, precision: usize) {
    append_number(buf, point.x, precision);
    buf.push(',');
    append_number(buf, point.y, precision);
}

fn append_number(buf: &mut String, value: f64, precision: usize) {
    let _ = write!(buf, "{value:.p$}", p = precision);
}

fn squared_distance(a: Point, b: Point) -> f64 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}

fn font_store() -> &'static FontStore {
    static STORE: OnceLock<FontStore> = OnceLock::new();
    STORE.get_or_init(FontStore::default)
}

#[derive(Default)]
struct FontStore {
    faces: RwLock<HashMap<String, Arc<OwnedFace>>>,
}

impl FontStore {
    fn face_for(&self, font: &Font) -> Result<Arc<OwnedFace>, FontLoadError> {
        let handle = FontDatabase::global()
            .resolve(font)
            .map_err(|err| FontLoadError::Resolve(err.to_string()))?;
        let key = handle.key.clone();

        if let Some(face) = self.faces.read().get(&key) {
            return Ok(face.clone());
        }

        let face = OwnedFace::from_vec(handle.bytes.as_ref().to_vec(), handle.face_index)
            .map_err(|source| FontLoadError::Parse { source })?;
        let face = Arc::new(face);

        self.faces.write().insert(key, face.clone());
        Ok(face)
    }
}

#[derive(Debug, Error)]
enum FontLoadError {
    #[error("failed to resolve font: {0}")]
    Resolve(String),
    #[error("invalid font data: {source}")]
    Parse {
        #[source]
        source: FaceParsingError,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use o4e_core::types::Direction;
    use std::fs;
    use std::path::PathBuf;

    fn sample_shaping_result() -> ShapingResult {
        ShapingResult {
            text: "ab".to_string(),
            glyphs: vec![
                Glyph {
                    id: 1,
                    cluster: 0,
                    x: 0.0,
                    y: 0.0,
                    advance: 10.0,
                },
                Glyph {
                    id: 2,
                    cluster: 1,
                    x: 10.0,
                    y: 0.0,
                    advance: 12.0,
                },
            ],
            advance: 22.0,
            bbox: BoundingBox {
                x: 0.0,
                y: -1.0,
                width: 22.0,
                height: 2.0,
            },
            font: None,
            direction: Direction::LeftToRight,
        }
    }

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
            text: String::new(),
            glyphs: vec![],
            advance: 0.0,
            bbox: BoundingBox {
                x: 0.0,
                y: 0.0,
                width: 100.0,
                height: 20.0,
            },
            font: None,
            direction: Direction::LeftToRight,
        };

        let svg = renderer.render(&shaped, &SvgOptions::default());
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
    }

    #[test]
    fn test_path_simplification() {
        let mut path = BezPath::new();
        path.move_to(Point::new(0.0, 0.0));
        path.line_to(Point::new(0.0001, 0.0));
        path.line_to(Point::new(10.0, 0.0));
        let simplified = simplify_path(path, 2);
        assert!(
            simplified.elements().len() < 3,
            "Simplification should drop near-zero segment"
        );
    }

    #[test]
    fn test_render_simple_text_produces_rectangles() {
        let renderer = SvgRenderer::default();
        let shaped = sample_shaping_result();
        let svg = renderer.render(&shaped, &SvgOptions::default());
        assert!(
            svg.contains("<rect"),
            "SVG should contain fallback rectangles"
        );
    }

    #[test]
    fn test_render_complex_positions_adjust_viewbox() {
        let renderer = SvgRenderer::default();
        let mut shaped = sample_shaping_result();
        shaped.glyphs[0].x = -5.0;
        shaped.glyphs[1].x = 15.0;
        let svg = renderer.render(&shaped, &SvgOptions::default());
        let expected = format!("viewBox=\"{:.2} {:.2} {:.2} {:.2}\"", -5.0, -1.0, 32.0, 1.5);
        assert!(
            svg.contains(&expected),
            "ViewBox should match calculated bounding box ({expected}), got {svg}"
        );
    }

    #[test]
    fn test_svg_output_is_well_formed() {
        let renderer = SvgRenderer::default();
        let svg = renderer.render(&sample_shaping_result(), &SvgOptions::default());
        assert!(svg.starts_with("<svg "), "SVG should start with root tag");
        assert!(svg.contains("</g>"), "SVG should close group tag");
        assert!(
            svg.trim_end().ends_with("</svg>"),
            "SVG should end with closing tag"
        );
    }

    #[test]
    fn test_render_includes_paths_when_font_available() {
        let renderer = SvgRenderer::default();
        let (font, path) = noto_sans_font(32.0);
        let glyph_id = glyph_id_for('A', &path);
        let glyph = Glyph {
            id: glyph_id,
            cluster: 0,
            x: 0.0,
            y: 0.0,
            advance: 24.0,
        };
        let shaped = ShapingResult {
            text: "A".into(),
            glyphs: vec![glyph],
            advance: 24.0,
            bbox: BoundingBox {
                x: 0.0,
                y: -32.0,
                width: 24.0,
                height: 48.0,
            },
            font: Some(font),
            direction: Direction::LeftToRight,
        };

        let svg = renderer.render(&shaped, &SvgOptions::default());
        assert!(
            svg.contains("<path id=\"glyph-0\""),
            "Expected glyph path in SVG output: {svg}"
        );
    }

    fn noto_sans_font(size: f32) -> (Font, PathBuf) {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fonts/NotoSans-Regular.ttf");
        let mut font = Font::new("Noto Sans".to_string(), size); // Corrected family name
        font.source = o4e_core::types::FontSource::Path(path.to_string_lossy().into_owned()); // Explicitly set source path
        (font, path)
    }

    fn glyph_id_for(ch: char, font_path: &PathBuf) -> u32 {
        let data = fs::read(font_path).expect("Test font readable");
        let face = OwnedFace::from_vec(data, 0).expect("Font parsed");
        face.as_face_ref()
            .glyph_index(ch)
            .expect("Glyph must exist")
            .0 as u32
    }
}
