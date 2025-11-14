// this_file: crates/o4e-render/src/outlines.rs

//! Shared glyph outline recording utilities.

use kurbo::{BezPath, Point};
use owned_ttf_parser::{AsFaceRef, OwnedFace};
use ttf_parser::{GlyphId, OutlineBuilder};

/// Recorded outline commands for a glyph.
#[derive(Debug, Clone, PartialEq)]
pub enum OutlineCommand {
    MoveTo(f32, f32),
    LineTo(f32, f32),
    QuadTo {
        ctrl_x: f32,
        ctrl_y: f32,
        x: f32,
        y: f32,
    },
    CurveTo {
        ctrl1_x: f32,
        ctrl1_y: f32,
        ctrl2_x: f32,
        ctrl2_y: f32,
        x: f32,
        y: f32,
    },
    Close,
}

/// Geometry container for a recorded glyph outline.
#[derive(Debug, Clone, Default)]
pub struct GlyphOutline {
    commands: Vec<OutlineCommand>,
}

impl GlyphOutline {
    pub fn commands(&self) -> &[OutlineCommand] {
        &self.commands
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    /// Convert the recorded outline into a `kurbo::BezPath`, applying the provided font-scale.
    pub fn to_bez_path(&self, scale: f32) -> BezPath {
        if self.commands.is_empty() || scale <= 0.0 {
            return BezPath::new();
        }

        let mut path = BezPath::new();
        for command in &self.commands {
            match *command {
                OutlineCommand::MoveTo(x, y) => path.move_to(scale_point(x, y, scale)),
                OutlineCommand::LineTo(x, y) => path.line_to(scale_point(x, y, scale)),
                OutlineCommand::QuadTo {
                    ctrl_x,
                    ctrl_y,
                    x,
                    y,
                } => path.quad_to(scale_point(ctrl_x, ctrl_y, scale), scale_point(x, y, scale)),
                OutlineCommand::CurveTo {
                    ctrl1_x,
                    ctrl1_y,
                    ctrl2_x,
                    ctrl2_y,
                    x,
                    y,
                } => path.curve_to(
                    scale_point(ctrl1_x, ctrl1_y, scale),
                    scale_point(ctrl2_x, ctrl2_y, scale),
                    scale_point(x, y, scale),
                ),
                OutlineCommand::Close => path.close_path(),
            }
        }

        path
    }
}

fn scale_point(x: f32, y: f32, scale: f32) -> Point {
    Point::new((x as f64) * (scale as f64), -(y as f64) * (scale as f64))
}

/// Types that can expose glyph outlines via ttf-parser builders.
pub trait OutlineSource {
    fn outline_with_builder<B: OutlineBuilder>(
        &self,
        glyph_id: GlyphId,
        builder: &mut B,
    ) -> Option<()>;
}

impl<'a> OutlineSource for ttf_parser::Face<'a> {
    fn outline_with_builder<B: OutlineBuilder>(
        &self,
        glyph_id: GlyphId,
        builder: &mut B,
    ) -> Option<()> {
        self.outline_glyph(glyph_id, builder).map(|_| ())
    }
}

impl OutlineSource for OwnedFace {
    fn outline_with_builder<B: OutlineBuilder>(
        &self,
        glyph_id: GlyphId,
        builder: &mut B,
    ) -> Option<()> {
        self.as_face_ref()
            .outline_glyph(glyph_id, builder)
            .map(|_| ())
    }
}

/// Record the outline for the provided glyph.
pub fn glyph_outline<S: OutlineSource>(source: &S, glyph_id: GlyphId) -> Option<GlyphOutline> {
    let mut recorder = RecordingOutline::default();
    source.outline_with_builder(glyph_id, &mut recorder)?;
    let outline = recorder.finish();
    (!outline.is_empty()).then_some(outline)
}

/// Convenience helper that records and converts a glyph outline to a `BezPath`.
pub fn glyph_bez_path<S: OutlineSource>(
    source: &S,
    glyph_id: GlyphId,
    scale: f32,
) -> Option<BezPath> {
    if scale <= 0.0 {
        return None;
    }
    glyph_outline(source, glyph_id).map(|outline| outline.to_bez_path(scale))
}

#[derive(Default)]
struct RecordingOutline {
    commands: Vec<OutlineCommand>,
}

impl RecordingOutline {
    fn finish(self) -> GlyphOutline {
        GlyphOutline {
            commands: self.commands,
        }
    }
}

impl OutlineBuilder for RecordingOutline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::MoveTo(x, y));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.commands.push(OutlineCommand::LineTo(x, y));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.commands.push(OutlineCommand::QuadTo {
            ctrl_x: x1,
            ctrl_y: y1,
            x,
            y,
        });
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.commands.push(OutlineCommand::CurveTo {
            ctrl1_x: x1,
            ctrl1_y: y1,
            ctrl2_x: x2,
            ctrl2_y: y2,
            x,
            y,
        });
    }

    fn close(&mut self) {
        self.commands.push(OutlineCommand::Close);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::Shape;
    use owned_ttf_parser::OwnedFace;
    use std::fs;
    use std::path::PathBuf;

    #[test]
    fn captures_outline_for_known_glyph() {
        let face = noto_face();
        let glyph_id = face
            .as_face_ref()
            .glyph_index('A')
            .expect("Noto Sans should include 'A'");
        let outline = glyph_outline(face.as_face_ref(), glyph_id).expect("outline recorded");
        assert!(outline.commands().len() > 4, "expected multiple commands");
    }

    #[test]
    fn records_quadratic_and_cubic_segments() {
        let face = noto_face();
        let glyph_id = face
            .as_face_ref()
            .glyph_index('g')
            .expect("Glyph must exist");
        let outline = glyph_outline(face.as_face_ref(), glyph_id).expect("outline recorded");
        assert!(
            outline
                .commands()
                .iter()
                .any(|cmd| matches!(cmd, OutlineCommand::QuadTo { .. })),
            "TrueType outlines should include quadratic segments"
        );
    }

    #[test]
    fn converts_outline_into_bez_path() {
        let face = noto_face();
        let glyph_id = face.as_face_ref().glyph_index('A').unwrap();
        let path = glyph_bez_path(face.as_face_ref(), glyph_id, 32.0).expect("path");
        assert!(
            !path.elements().is_empty(),
            "conversion should emit bezier elements"
        );
        let bounds = path.bounding_box();
        assert!(bounds.width() > 0.0 && bounds.height() > 0.0);
    }

    fn noto_face() -> OwnedFace {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../testdata/fonts/NotoSans-Regular.ttf");
        let data = fs::read(&path).expect("Test font readable");
        OwnedFace::from_vec(data, 0).expect("Test font parsed")
    }
}
