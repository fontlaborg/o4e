// this_file: src/output.rs

//! Image output generation (PGM and PNG formats).
//!
//! This module generates PGM (grayscale) and PNG images from rendered pixel data,
//! with base64 encoding for JSONL output.

use crate::error::{Error, Result};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use image::{ImageBuffer, Luma};
use std::io::{Read, Write};

/// Image output format handler.
pub struct ImageOutput;

impl ImageOutput {
    /// Generate PGM P5 (binary) format from grayscale pixels.
    ///
    /// PGM format:
    /// ```text
    /// P5
    /// <width> <height>
    /// 255
    /// <binary pixel data>
    /// ```
    pub fn write_pgm_binary(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        if pixels.len() != (width * height) as usize {
            return Err(Error::Internal(format!(
                "Pixel data size mismatch: expected {} bytes, got {}",
                width * height,
                pixels.len()
            )));
        }

        let mut output = Vec::new();

        // Write PGM header
        writeln!(&mut output, "P5")?;
        writeln!(&mut output, "{} {}", width, height)?;
        writeln!(&mut output, "255")?;

        // Write binary pixel data
        output.extend_from_slice(pixels);

        Ok(output)
    }

    /// Generate PNG format from grayscale pixels.
    pub fn write_png(pixels: &[u8], width: u32, height: u32) -> Result<Vec<u8>> {
        if pixels.len() != (width * height) as usize {
            return Err(Error::Internal(format!(
                "Pixel data size mismatch: expected {} bytes, got {}",
                width * height,
                pixels.len()
            )));
        }

        // Create image buffer
        let img: ImageBuffer<Luma<u8>, Vec<u8>> =
            ImageBuffer::from_raw(width, height, pixels.to_vec()).ok_or_else(|| {
                Error::Internal("Failed to create image buffer from pixels".to_string())
            })?;

        // Encode as PNG
        let mut output = Vec::new();
        img.write_to(
            &mut std::io::Cursor::new(&mut output),
            image::ImageFormat::Png,
        )
        .map_err(|e| Error::ImageEncode(e))?;

        Ok(output)
    }

    /// Base64-encode image data for JSONL output.
    pub fn encode_base64(data: &[u8]) -> String {
        BASE64.encode(data)
    }

    /// Decode base64-encoded image data (for testing).
    #[cfg(test)]
    pub fn decode_base64(encoded: &str) -> Result<Vec<u8>> {
        BASE64
            .decode(encoded)
            .map_err(|e| Error::Internal(format!("Base64 decode error: {}", e)))
    }

    /// Decode PGM P5 format (for testing).
    #[cfg(test)]
    pub fn decode_pgm(data: &[u8]) -> Result<(Vec<u8>, u32, u32)> {
        use std::io::{BufRead, BufReader};

        let mut reader = BufReader::new(data);
        let mut line = String::new();

        // Read "P5"
        reader.read_line(&mut line)?;
        if line.trim() != "P5" {
            return Err(Error::Internal(format!(
                "Invalid PGM format: expected 'P5', got '{}'",
                line.trim()
            )));
        }

        // Read width and height
        line.clear();
        reader.read_line(&mut line)?;
        let parts: Vec<&str> = line.trim().split_whitespace().collect();
        if parts.len() != 2 {
            return Err(Error::Internal("Invalid PGM dimensions".to_string()));
        }
        let width: u32 = parts[0]
            .parse()
            .map_err(|_| Error::Internal(format!("Invalid width: {}", parts[0])))?;
        let height: u32 = parts[1]
            .parse()
            .map_err(|_| Error::Internal(format!("Invalid height: {}", parts[1])))?;

        // Read maxval (should be 255)
        line.clear();
        reader.read_line(&mut line)?;
        let maxval: u32 = line
            .trim()
            .parse()
            .map_err(|_| Error::Internal(format!("Invalid maxval: {}", line.trim())))?;
        if maxval != 255 {
            return Err(Error::Internal(format!(
                "Unsupported maxval: {} (expected 255)",
                maxval
            )));
        }

        // Read binary pixel data
        let mut pixels = Vec::new();
        reader.read_to_end(&mut pixels)?;

        if pixels.len() != (width * height) as usize {
            return Err(Error::Internal(format!(
                "Pixel data size mismatch: expected {} bytes, got {}",
                width * height,
                pixels.len()
            )));
        }

        Ok((pixels, width, height))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_pgm_binary() {
        let pixels = vec![0u8, 128, 255, 64];
        let pgm = ImageOutput::write_pgm_binary(&pixels, 2, 2).unwrap();

        // Check header (P5\n2 2\n255\n = 11 bytes)
        let header = String::from_utf8_lossy(&pgm[..11]);
        assert!(header.starts_with("P5"));
        assert!(header.contains("2 2"));
        assert!(header.contains("255"));

        // Check pixel data starts at byte 11
        assert_eq!(&pgm[11..], &pixels);
    }

    #[test]
    fn test_pgm_round_trip() {
        let original_pixels = vec![0u8, 50, 100, 150, 200, 255];
        let pgm = ImageOutput::write_pgm_binary(&original_pixels, 3, 2).unwrap();

        let (decoded_pixels, width, height) = ImageOutput::decode_pgm(&pgm).unwrap();
        assert_eq!(width, 3);
        assert_eq!(height, 2);
        assert_eq!(decoded_pixels, original_pixels);
    }

    #[test]
    fn test_base64_round_trip() {
        let data = b"Hello, Haforu!";
        let encoded = ImageOutput::encode_base64(data);
        let decoded = ImageOutput::decode_base64(&encoded).unwrap();
        assert_eq!(decoded, data);
    }

    #[test]
    fn test_write_png() {
        let pixels = vec![0u8; 100 * 50]; // 100×50 black image
        let png = ImageOutput::write_png(&pixels, 100, 50).unwrap();

        // Check PNG signature
        assert_eq!(&png[0..8], b"\x89PNG\r\n\x1a\n");
    }

    #[test]
    fn test_write_pgm_size_mismatch() {
        let pixels = vec![0u8; 10];
        let result = ImageOutput::write_pgm_binary(&pixels, 100, 50);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("size mismatch"));
    }
}
