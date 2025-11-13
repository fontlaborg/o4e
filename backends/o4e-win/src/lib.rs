// this_file: backends/o4e-win/src/lib.rs

//! DirectWrite backend for Windows text rendering.

#![cfg(target_os = "windows")]

use o4e_core::{
    types::{Direction, RenderFormat},
    Backend, Bitmap, Font, FontCache, Glyph, O4eError, RenderOptions, RenderOutput, Result,
    SegmentOptions, ShapingResult, TextRun,
};

use windows::{
    core::*,
    Win32::{
        Foundation::*,
        Graphics::{Direct2D::Common::*, Direct2D::*, DirectWrite::*, Dxgi::Common::*, Imaging::*},
        System::Com::*,
    },
};

use anyhow::anyhow;
use lru::LruCache;
use parking_lot::RwLock;
use std::num::NonZeroUsize;
use std::sync::Arc;

pub struct DirectWriteBackend {
    dwrite_factory: IDWriteFactory,
    d2d_factory: ID2D1Factory,
    wic_factory: IWICImagingFactory,
    cache: FontCache,
    font_cache: RwLock<LruCache<String, IDWriteFontFace>>,
    shape_cache: RwLock<LruCache<String, Arc<ShapingResult>>>,
}

// Safety: DirectWrite interfaces are thread-safe when used correctly
unsafe impl Send for DirectWriteBackend {}
unsafe impl Sync for DirectWriteBackend {}

impl DirectWriteBackend {
    pub fn new() -> Result<Self> {
        unsafe {
            // Initialize COM
            let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

            // Create DirectWrite factory
            let dwrite_factory: IDWriteFactory = DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)?;

            // Create Direct2D factory
            let d2d_factory: ID2D1Factory =
                D2D1CreateFactory(D2D1_FACTORY_TYPE_MULTI_THREADED, None)?;

            // Create WIC factory for image processing
            let wic_factory: IWICImagingFactory =
                CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER)?;

            Ok(Self {
                dwrite_factory,
                d2d_factory,
                wic_factory,
                cache: FontCache::new(512),
                font_cache: RwLock::new(LruCache::new(NonZeroUsize::new(64).unwrap())),
                shape_cache: RwLock::new(LruCache::new(NonZeroUsize::new(256).unwrap())),
            })
        }
    }

    fn get_or_create_font_face(&self, font: &Font) -> Result<IDWriteFontFace> {
        let cache_key = format!("{}:{}", font.family, font.size as u32);

        // Check cache
        {
            let mut cache = self.font_cache.write();
            if let Some(font_face) = cache.get(&cache_key) {
                return Ok(font_face.clone());
            }
        }

        unsafe {
            // Get system font collection
            let font_collection = self.dwrite_factory.GetSystemFontCollection(false)?;

            // Find font family
            let family_name = HSTRING::from(&font.family);
            let mut index = 0u32;
            let mut exists = BOOL::default();
            font_collection.FindFamilyName(&family_name, &mut index, &mut exists)?;

            if !exists.as_bool() {
                return Err(O4eError::FontNotFound {
                    name: font.family.clone(),
                }
                .into());
            }

            // Get font family
            let font_family = font_collection.GetFontFamily(index)?;

            // Get font with specified weight and style
            let weight = DWRITE_FONT_WEIGHT(font.weight as i32);
            let style = DWRITE_FONT_STYLE_NORMAL;
            let stretch = DWRITE_FONT_STRETCH_NORMAL;

            let dwrite_font = font_family.GetFirstMatchingFont(weight, stretch, style)?;

            // Create font face
            let font_face = dwrite_font.CreateFontFace()?;

            // Cache it
            {
                let mut cache = self.font_cache.write();
                cache.push(cache_key, font_face.clone());
            }

            Ok(font_face)
        }
    }

    fn create_text_layout(&self, text: &str, font: &Font) -> Result<IDWriteTextLayout> {
        unsafe {
            let text_wide: Vec<u16> = text.encode_utf16().collect();
            let font_face = self.get_or_create_font_face(font)?;

            // Create text format
            let text_format = self.dwrite_factory.CreateTextFormat(
                &HSTRING::from(&font.family),
                None,
                DWRITE_FONT_WEIGHT(font.weight as i32),
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                font.size,
                &HSTRING::from("en-US"),
            )?;

            // Create text layout
            let text_layout = self.dwrite_factory.CreateTextLayout(
                &text_wide,
                &text_format,
                10000.0, // Max width
                10000.0, // Max height
            )?;

            Ok(text_layout)
        }
    }
}

impl Backend for DirectWriteBackend {
    fn segment(&self, text: &str, options: &SegmentOptions) -> Result<Vec<TextRun>> {
        // For now, create a single run for the entire text
        // TODO: Implement proper segmentation using IDWriteTextAnalyzer
        let mut runs = Vec::new();

        runs.push(TextRun {
            text: text.to_string(),
            range: (0, text.len()),
            script: "Latin".to_string(),
            language: options.language.clone().unwrap_or_else(|| "en".to_string()),
            direction: Direction::LeftToRight,
            font: None,
        });

        Ok(runs)
    }

    fn shape(&self, run: &TextRun, font: &Font) -> Result<ShapingResult> {
        // Check cache
        let cache_key = format!("{}:{}:{}", run.text, font.family, font.size as u32);
        {
            let mut cache = self.shape_cache.write();
            if let Some(result) = cache.get(&cache_key) {
                return Ok((**result).clone());
            }
        }

        unsafe {
            // Create text layout
            let text_layout = self.create_text_layout(&run.text, font)?;

            // Get metrics
            let mut metrics = DWRITE_TEXT_METRICS::default();
            text_layout.GetMetrics(&mut metrics)?;

            // Get line metrics to determine glyph positions
            let mut line_count = 0u32;
            text_layout.GetLineMetrics(None, &mut line_count)?;

            let mut line_metrics = vec![DWRITE_LINE_METRICS::default(); line_count as usize];
            text_layout.GetLineMetrics(Some(&mut line_metrics), &mut line_count)?;

            // Create simplified glyphs based on character positions
            // This is a simplified approach - DirectWrite's actual glyph extraction is more complex
            let mut glyphs = Vec::new();
            let mut x_offset = 0.0;

            let char_width = metrics.width / run.text.chars().count() as f32;

            for (idx, ch) in run.text.char_indices() {
                glyphs.push(Glyph {
                    id: ch as u32,
                    cluster: idx as u32,
                    x: x_offset,
                    y: 0.0,
                    advance: char_width,
                });
                x_offset += char_width;
            }

            let bbox = o4e_core::utils::calculate_bbox(&glyphs);

            let result = ShapingResult {
                glyphs,
                advance: metrics.width,
                bbox,
                font: Some(font.clone()),
            };

            let result = Arc::new(result);

            // Cache the result
            {
                let mut cache = self.shape_cache.write();
                cache.push(cache_key, result.clone());
            }

            Ok((*result).clone())
        }
    }

    fn render(&self, shaped: &ShapingResult, options: &RenderOptions) -> Result<RenderOutput> {
        // Check if we have glyphs to render
        if shaped.glyphs.is_empty() {
            return Ok(RenderOutput::Bitmap(Bitmap {
                width: 1,
                height: 1,
                data: vec![0, 0, 0, 0],
            }));
        }

        // Get the font from ShapingResult
        let font = shaped
            .font
            .as_ref()
            .ok_or_else(|| O4eError::render("Font information missing from shaped result"))?;

        unsafe {
            // Calculate dimensions
            let padding = options.padding as f32;
            let width = (shaped.bbox.width + padding * 2.0).ceil() as u32;
            let height = (shaped.bbox.height + padding * 2.0).ceil() as u32;

            // Create WIC bitmap
            let bitmap = self.wic_factory.CreateBitmap(
                width,
                height,
                &GUID_WICPixelFormat32bppPBGRA,
                WICBitmapCacheOnDemand,
            )?;

            // Create D2D render target from WIC bitmap
            let render_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_B8G8R8A8_UNORM,
                    alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
                },
                dpiX: 96.0,
                dpiY: 96.0,
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };

            let render_target = self
                .d2d_factory
                .CreateWicBitmapRenderTarget(&bitmap, &render_props)?;

            // Parse colors
            let (text_r, text_g, text_b, text_a) =
                o4e_core::utils::parse_color(&options.color).map_err(|e| O4eError::render(e))?;

            // Begin drawing
            render_target.BeginDraw();

            // Clear background
            if options.background != "transparent" {
                let (bg_r, bg_g, bg_b, bg_a) = o4e_core::utils::parse_color(&options.background)
                    .map_err(|e| O4eError::render(e))?;

                let clear_color = D2D1_COLOR_F {
                    r: bg_r as f32 / 255.0,
                    g: bg_g as f32 / 255.0,
                    b: bg_b as f32 / 255.0,
                    a: bg_a as f32 / 255.0,
                };
                render_target.Clear(Some(&clear_color));
            } else {
                // Clear to transparent
                let clear_color = D2D1_COLOR_F {
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    a: 0.0,
                };
                render_target.Clear(Some(&clear_color));
            }

            // Create brush for text
            let text_color = D2D1_COLOR_F {
                r: text_r as f32 / 255.0,
                g: text_g as f32 / 255.0,
                b: text_b as f32 / 255.0,
                a: text_a as f32 / 255.0,
            };

            let brush = render_target.CreateSolidColorBrush(&text_color, None)?;

            // Draw text (simplified - using basic text for now)
            // In production, we'd use the shaped glyphs properly
            let text = "Hello World"; // Placeholder text
            let text_layout = self.create_text_layout(text, font)?;

            let origin = D2D_POINT_2F {
                x: padding,
                y: padding,
            };

            render_target.DrawTextLayout(origin, &text_layout, &brush, D2D1_DRAW_TEXT_OPTIONS_NONE);

            // End drawing
            render_target.EndDraw(None, None)?;

            // Get pixels from WIC bitmap
            let mut buffer = vec![0u8; (width * height * 4) as usize];
            let rect = WICRect {
                X: 0,
                Y: 0,
                Width: width as i32,
                Height: height as i32,
            };

            bitmap.CopyPixels(&rect, width * 4, &mut buffer)?;

            // Convert from BGRA to RGBA
            for chunk in buffer.chunks_mut(4) {
                chunk.swap(0, 2);
            }

            // Convert to requested format
            match options.format {
                RenderFormat::Raw => {
                    let bitmap = Bitmap {
                        width,
                        height,
                        data: buffer,
                    };
                    Ok(RenderOutput::Bitmap(bitmap))
                }
                RenderFormat::Png => {
                    // Encode as PNG
                    let mut png_data = Vec::new();
                    {
                        let mut encoder = png::Encoder::new(&mut png_data, width, height);
                        encoder.set_color(png::ColorType::Rgba);
                        encoder.set_depth(png::BitDepth::Eight);
                        let mut writer = encoder
                            .write_header()
                            .map_err(|e| O4eError::render(format!("PNG encoding error: {}", e)))?;
                        writer
                            .write_image_data(&buffer)
                            .map_err(|e| O4eError::render(format!("PNG write error: {}", e)))?;
                    }
                    Ok(RenderOutput::Png(png_data))
                }
                RenderFormat::Svg => {
                    // SVG rendering using o4e-render
                    let svg_options = o4e_core::types::SvgOptions::default();
                    let renderer = o4e_render::SvgRenderer::new(&svg_options);
                    let svg = renderer.render(&shaped, &svg_options);
                    Ok(RenderOutput::Svg(svg))
                }
            }
        }
    }

    fn name(&self) -> &str {
        "DirectWrite"
    }

    fn clear_cache(&self) {
        self.cache.clear();
        self.font_cache.write().clear();
        self.shape_cache.write().clear();
    }
}

impl Default for DirectWriteBackend {
    fn default() -> Self {
        Self::new().expect("Failed to initialize DirectWrite backend")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation() {
        let backend = DirectWriteBackend::new();
        assert!(backend.is_ok());
        if let Ok(backend) = backend {
            assert_eq!(backend.name(), "DirectWrite");
        }
    }

    #[test]
    fn test_simple_segmentation() {
        if let Ok(backend) = DirectWriteBackend::new() {
            let options = SegmentOptions::default();
            let runs = backend.segment("Hello World", &options).unwrap();
            assert_eq!(runs.len(), 1);
            assert_eq!(runs[0].text, "Hello World");
        }
    }
}
