// this_file: tests/integration.rs

//! Integration tests for o4e text rendering engine.

use o4e_core::{Backend, Font, RenderOptions, SegmentOptions};

#[cfg(target_os = "macos")]
use o4e_mac::CoreTextBackend;

#[cfg(target_os = "windows")]
use o4e_win::DirectWriteBackend;

use o4e_icu_hb::HarfBuzzBackend;

/// Get all available backends for the current platform
fn get_available_backends() -> Vec<Box<dyn Backend>> {
    let mut backends: Vec<Box<dyn Backend>> = vec![];

    // Always include HarfBuzz backend
    backends.push(Box::new(HarfBuzzBackend::new()));

    // Platform-specific backends
    #[cfg(target_os = "macos")]
    {
        backends.push(Box::new(CoreTextBackend::new()));
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(backend) = DirectWriteBackend::new() {
            backends.push(Box::new(backend));
        }
    }

    backends
}

#[test]
fn test_backend_initialization() {
    let backends = get_available_backends();
    assert!(!backends.is_empty(), "At least one backend should be available");

    for backend in backends {
        println!("Testing backend: {}", backend.name());
        assert!(!backend.name().is_empty());
    }
}

#[test]
fn test_simple_latin_text() {
    let backends = get_available_backends();
    let font = Font::new("Arial", 24.0);
    let text = "Hello World";
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    for backend in backends {
        println!("Testing Latin text with backend: {}", backend.name());

        // Segment text
        let runs = backend.segment(text, &segment_options).unwrap();
        assert!(!runs.is_empty(), "Should produce at least one text run");

        // Shape text
        for run in &runs {
            let shaped = backend.shape(run, &font).unwrap();
            assert!(!shaped.glyphs.is_empty(), "Should produce glyphs");
            assert!(shaped.advance > 0.0, "Should have positive advance");

            // Render text
            let rendered = backend.render(&shaped, &render_options).unwrap();
            match rendered {
                o4e_core::RenderOutput::Bitmap(bitmap) => {
                    assert!(bitmap.width > 0);
                    assert!(bitmap.height > 0);
                    assert!(!bitmap.data.is_empty());
                }
                _ => {}
            }
        }
    }
}

#[test]
fn test_unicode_scripts() {
    let backends = get_available_backends();
    let font = Font::new("NotoSans", 24.0);
    let segment_options = SegmentOptions::default();

    let test_cases = vec![
        ("Hello", "Latin"),
        ("Привет", "Cyrillic"),
        ("Γειά", "Greek"),
        ("שלום", "Hebrew"),
        ("مرحبا", "Arabic"),
        ("你好", "CJK"),
        ("こんにちは", "Japanese"),
        ("안녕하세요", "Korean"),
    ];

    for backend in backends {
        println!("Testing Unicode scripts with backend: {}", backend.name());

        for (text, script_name) in &test_cases {
            println!("  Testing {} script: {}", script_name, text);

            // Segment text
            let runs = backend.segment(text, &segment_options);

            // We may not support all scripts yet, so just check it doesn't panic
            if let Ok(runs) = runs {
                assert!(!runs.is_empty(), "Should produce at least one text run for {}", script_name);

                // Try shaping
                for run in &runs {
                    if let Ok(shaped) = backend.shape(run, &font) {
                        assert!(!shaped.glyphs.is_empty(), "Should produce glyphs for {}", script_name);
                    }
                }
            }
        }
    }
}

#[test]
fn test_font_sizes() {
    let backends = get_available_backends();
    let text = "Test";
    let segment_options = SegmentOptions::default();
    let sizes = vec![8.0, 12.0, 16.0, 24.0, 36.0, 48.0, 72.0, 144.0];

    for backend in backends {
        println!("Testing font sizes with backend: {}", backend.name());

        let runs = backend.segment(text, &segment_options).unwrap();
        let run = &runs[0];

        let mut previous_advance = 0.0;

        for size in &sizes {
            let font = Font::new("Arial", *size);
            let shaped = backend.shape(run, &font).unwrap();

            // Larger font sizes should generally have larger advances
            if previous_advance > 0.0 {
                assert!(
                    shaped.advance >= previous_advance * 0.8,
                    "Font size {} should have advance >= {} (got {})",
                    size,
                    previous_advance * 0.8,
                    shaped.advance
                );
            }
            previous_advance = shaped.advance;
        }
    }
}

#[test]
fn test_empty_text() {
    let backends = get_available_backends();
    let font = Font::new("Arial", 24.0);
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    for backend in backends {
        println!("Testing empty text with backend: {}", backend.name());

        // Empty string should be handled gracefully
        let runs = backend.segment("", &segment_options).unwrap();

        if !runs.is_empty() {
            for run in &runs {
                let shaped = backend.shape(run, &font).unwrap();
                let rendered = backend.render(&shaped, &render_options).unwrap();

                // Even empty text should produce valid output
                match rendered {
                    o4e_core::RenderOutput::Bitmap(bitmap) => {
                        assert!(bitmap.width >= 1);
                        assert!(bitmap.height >= 1);
                    }
                    _ => {}
                }
            }
        }
    }
}

#[test]
fn test_special_characters() {
    let backends = get_available_backends();
    let font = Font::new("Arial", 24.0);
    let segment_options = SegmentOptions::default();

    let test_cases = vec![
        "Hello\nWorld",     // Newline
        "Hello\tWorld",     // Tab
        "Hello World",      // Multiple spaces
        "Hello!@#$%^&*()",  // Special symbols
        "\"Hello\"",        // Quotes
        "Hello—World",      // Em dash
        "Hello…",           // Ellipsis
        "💖🎉🚀",          // Emoji
    ];

    for backend in backends {
        println!("Testing special characters with backend: {}", backend.name());

        for text in &test_cases {
            println!("  Testing: {:?}", text);

            // Should not panic
            if let Ok(runs) = backend.segment(text, &segment_options) {
                for run in &runs {
                    let _ = backend.shape(run, &font);
                }
            }
        }
    }
}

#[test]
fn test_caching() {
    let backends = get_available_backends();
    let font = Font::new("Arial", 24.0);
    let text = "Cached text";
    let segment_options = SegmentOptions::default();

    for backend in backends {
        println!("Testing caching with backend: {}", backend.name());

        let runs = backend.segment(text, &segment_options).unwrap();
        let run = &runs[0];

        // Shape the same text multiple times
        let start = std::time::Instant::now();
        let shaped1 = backend.shape(run, &font).unwrap();
        let first_time = start.elapsed();

        let start = std::time::Instant::now();
        let shaped2 = backend.shape(run, &font).unwrap();
        let second_time = start.elapsed();

        // Second time should be faster due to caching
        // (though this isn't guaranteed on all systems)
        println!(
            "  First shape: {:?}, Second shape: {:?}",
            first_time, second_time
        );

        // Results should be identical
        assert_eq!(shaped1.glyphs.len(), shaped2.glyphs.len());
        assert_eq!(shaped1.advance, shaped2.advance);
    }
}

#[test]
fn test_render_formats() {
    let backends = get_available_backends();
    let font = Font::new("Arial", 24.0);
    let text = "Format Test";
    let segment_options = SegmentOptions::default();

    for backend in backends {
        println!("Testing render formats with backend: {}", backend.name());

        let runs = backend.segment(text, &segment_options).unwrap();
        let run = &runs[0];
        let shaped = backend.shape(run, &font).unwrap();

        // Test different formats
        let formats = vec![
            o4e_core::types::RenderFormat::Raw,
            o4e_core::types::RenderFormat::Png,
            o4e_core::types::RenderFormat::Svg,
        ];

        for format in formats {
            let mut options = RenderOptions::default();
            options.format = format;

            let rendered = backend.render(&shaped, &options).unwrap();

            match (format, rendered) {
                (o4e_core::types::RenderFormat::Raw, o4e_core::RenderOutput::Bitmap(bitmap)) => {
                    assert!(bitmap.width > 0);
                    assert!(bitmap.height > 0);
                    assert_eq!(bitmap.data.len(), (bitmap.width * bitmap.height * 4) as usize);
                }
                (o4e_core::types::RenderFormat::Png, o4e_core::RenderOutput::Png(data)) => {
                    assert!(!data.is_empty());
                    // PNG magic number
                    assert_eq!(&data[0..8], &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A]);
                }
                (o4e_core::types::RenderFormat::Svg, o4e_core::RenderOutput::Svg(svg)) => {
                    assert!(!svg.is_empty());
                    assert!(svg.contains("<svg"));
                    assert!(svg.contains("</svg>"));
                }
                _ => panic!("Format mismatch"),
            }
        }
    }
}