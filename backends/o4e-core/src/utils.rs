// this_file: backends/o4e-core/src/utils.rs

//! Utility functions for the o4e rendering engine.

use crate::types::{BoundingBox, Glyph, ShapingResult};

/// Calculate bounding box for a set of glyphs
pub fn calculate_bbox(glyphs: &[Glyph]) -> BoundingBox {
    if glyphs.is_empty() {
        return BoundingBox {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    }

    let mut min_x = f32::MAX;
    let mut min_y = f32::MAX;
    let mut max_x = f32::MIN;
    let mut max_y = f32::MIN;

    for glyph in glyphs {
        min_x = min_x.min(glyph.x);
        min_y = min_y.min(glyph.y);
        max_x = max_x.max(glyph.x + glyph.advance);
        max_y = max_y.max(glyph.y);
    }

    // Add some default height if we only have baseline info
    if (max_y - min_y).abs() < 0.001 {
        max_y = min_y + 100.0; // Default line height
    }

    BoundingBox {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

/// Combine multiple shaping results into one
pub fn combine_shaped_results(results: Vec<ShapingResult>) -> ShapingResult {
    let mut all_glyphs = Vec::new();
    let mut total_advance = 0.0;
    let mut x_offset = 0.0;

    for mut result in results {
        // Offset glyphs by accumulated advance
        for glyph in &mut result.glyphs {
            glyph.x += x_offset;
        }
        all_glyphs.extend(result.glyphs);
        total_advance += result.advance;
        x_offset += result.advance;
    }

    let bbox = calculate_bbox(&all_glyphs);

    ShapingResult {
        glyphs: all_glyphs,
        advance: total_advance,
        bbox,
        font: None, // Combined results don't have a single font
    }
}

/// Quantize font size for cache key generation
pub fn quantize_size(size: f32) -> u32 {
    (size * 100.0) as u32
}

/// Parse hex color string to RGBA
pub fn parse_color(color: &str) -> Result<(u8, u8, u8, u8), String> {
    if let Some(hex) = color.strip_prefix('#') {
        if hex.len() == 6 {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
            return Ok((r, g, b, 255));
        } else if hex.len() == 8 {
            let r = u8::from_str_radix(&hex[0..2], 16).map_err(|e| e.to_string())?;
            let g = u8::from_str_radix(&hex[2..4], 16).map_err(|e| e.to_string())?;
            let b = u8::from_str_radix(&hex[4..6], 16).map_err(|e| e.to_string())?;
            let a = u8::from_str_radix(&hex[6..8], 16).map_err(|e| e.to_string())?;
            return Ok((r, g, b, a));
        }
    }

    if color == "transparent" {
        return Ok((0, 0, 0, 0));
    }

    // Default to black
    Ok((0, 0, 0, 255))
}

/// System font directories for different platforms
pub fn system_font_dirs() -> Vec<String> {
    #[cfg(target_os = "macos")]
    {
        vec![
            "/System/Library/Fonts".to_string(),
            "/Library/Fonts".to_string(),
            "~/Library/Fonts".to_string(),
        ]
    }

    #[cfg(target_os = "windows")]
    {
        vec!["C:\\Windows\\Fonts".to_string()]
    }

    #[cfg(target_os = "linux")]
    {
        vec![
            "/usr/share/fonts".to_string(),
            "/usr/local/share/fonts".to_string(),
            "~/.fonts".to_string(),
            "~/.local/share/fonts".to_string(),
        ]
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
    {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_bbox() {
        let glyphs = vec![
            Glyph {
                id: 1,
                cluster: 0,
                x: 10.0,
                y: 20.0,
                advance: 15.0,
            },
            Glyph {
                id: 2,
                cluster: 1,
                x: 25.0,
                y: 20.0,
                advance: 10.0,
            },
        ];

        let bbox = calculate_bbox(&glyphs);
        assert_eq!(bbox.x, 10.0);
        assert_eq!(bbox.width, 25.0);
    }

    #[test]
    fn test_parse_color() {
        assert_eq!(parse_color("#FF0000").unwrap(), (255, 0, 0, 255));
        assert_eq!(parse_color("#00FF00FF").unwrap(), (0, 255, 0, 255));
        assert_eq!(parse_color("transparent").unwrap(), (0, 0, 0, 0));
    }

    #[test]
    fn test_quantize_size() {
        assert_eq!(quantize_size(12.5), 1250);
        assert_eq!(quantize_size(24.0), 2400);
    }
}
