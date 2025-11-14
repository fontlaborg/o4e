// this_file: backends/o4e-core/src/surface.rs

//! Shared helpers for converting backend-specific buffers into [`RenderOutput`]s.

use crate::{
    types::{Bitmap, RenderFormat, RenderOutput},
    O4eError, Result,
};

/// Raw pixel format for a render surface.
#[derive(Debug, Clone, Copy)]
pub enum SurfaceFormat {
    /// RGBA ordering.
    Rgba,
    /// BGRA ordering.
    Bgra,
    /// Grayscale alpha-less mask.
    Gray,
}

/// Render surface produced by a backend prior to format conversion/encoding.
#[derive(Debug)]
pub struct RenderSurface {
    width: u32,
    height: u32,
    format: SurfaceFormat,
    premultiplied: bool,
    data: Vec<u8>,
}

impl RenderSurface {
    /// Create a new RGBA surface.
    pub fn from_rgba(width: u32, height: u32, data: Vec<u8>, premultiplied: bool) -> Self {
        Self {
            width,
            height,
            format: SurfaceFormat::Rgba,
            premultiplied,
            data,
        }
    }

    /// Create a new BGRA surface.
    pub fn from_bgra(width: u32, height: u32, data: Vec<u8>, premultiplied: bool) -> Self {
        Self {
            width,
            height,
            format: SurfaceFormat::Bgra,
            premultiplied,
            data,
        }
    }

    /// Create a grayscale surface (used for alpha-only glyph caches).
    pub fn from_gray(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            format: SurfaceFormat::Gray,
            premultiplied: false,
            data,
        }
    }

    /// Convert the surface into a [`RenderOutput`].
    pub fn into_render_output(self, format: RenderFormat) -> Result<RenderOutput> {
        let width = self.width;
        let height = self.height;
        match format {
            RenderFormat::Svg => Err(O4eError::render(
                "RenderSurface cannot be converted to SVG output",
            )),
            RenderFormat::Raw => {
                let rgba = self.into_rgba_data()?;
                Ok(RenderOutput::Bitmap(Bitmap {
                    width,
                    height,
                    data: rgba,
                }))
            }
            RenderFormat::Png => {
                let rgba = self.into_rgba_data()?;
                let png_data = encode_png(width, height, &rgba)?;
                Ok(RenderOutput::Png(png_data))
            }
        }
    }

    fn into_rgba_data(mut self) -> Result<Vec<u8>> {
        match self.format {
            SurfaceFormat::Gray => Ok(expand_gray(&self.data)),
            SurfaceFormat::Rgba => {
                if self.premultiplied {
                    unpremultiply(&mut self.data);
                }
                Ok(std::mem::take(&mut self.data))
            }
            SurfaceFormat::Bgra => {
                bgra_to_rgba(&mut self.data);
                if self.premultiplied {
                    unpremultiply(&mut self.data);
                }
                Ok(std::mem::take(&mut self.data))
            }
        }
    }
}

fn expand_gray(data: &[u8]) -> Vec<u8> {
    let mut rgba = Vec::with_capacity(data.len() * 4);
    for &value in data {
        rgba.extend_from_slice(&[value, value, value, 255]);
    }
    rgba
}

fn bgra_to_rgba(data: &mut [u8]) {
    for chunk in data.chunks_exact_mut(4) {
        chunk.swap(0, 2);
    }
}

fn unpremultiply(data: &mut [u8]) {
    for chunk in data.chunks_exact_mut(4) {
        let alpha = chunk[3];
        if alpha == 0 || alpha == 255 {
            continue;
        }
        let alpha_f = alpha as f32 / 255.0;
        for channel in &mut chunk[..3] {
            let unpremultiplied = ((*channel as f32) / alpha_f).clamp(0.0, 255.0);
            *channel = unpremultiplied as u8;
        }
    }
}

fn encode_png(width: u32, height: u32, data: &[u8]) -> Result<Vec<u8>> {
    let mut png_data = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_data, width, height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|err| O4eError::render(format!("PNG encoder error: {err}")))?;
        writer
            .write_image_data(data)
            .map_err(|err| O4eError::render(format!("PNG write error: {err}")))?;
    } // writer and encoder are dropped here
    Ok(png_data)
}
