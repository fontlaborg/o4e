// this_file: benches/single_render.rs

//! Single render performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use o4e_core::{Backend, Font, RenderOptions, SegmentOptions};

#[cfg(target_os = "macos")]
use o4e_mac::CoreTextBackend;

use o4e_icu_hb::HarfBuzzBackend;

fn get_backend() -> Box<dyn Backend> {
    #[cfg(target_os = "macos")]
    {
        Box::new(CoreTextBackend::new())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Box::new(HarfBuzzBackend::new())
    }
}

fn bench_simple_latin(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let text = "The quick brown fox jumps over the lazy dog";
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    c.bench_function("simple_latin_render", |b| {
        b.iter(|| {
            let runs = backend.segment(black_box(text), &segment_options).unwrap();
            for run in &runs {
                let shaped = backend.shape(black_box(run), black_box(&font)).unwrap();
                let _ = backend
                    .render(black_box(&shaped), black_box(&render_options))
                    .unwrap();
            }
        });
    });
}

fn bench_complex_scripts(c: &mut Criterion) {
    let backend = get_backend();
    let font = Font::new("NotoSans", 24.0);
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    let test_cases = vec![
        ("Arabic", "مرحبا بالعالم هذا نص تجريبي"),
        ("Hebrew", "שלום עולם זהו טקסט לדוגמה"),
        ("CJK", "你好世界這是測試文本"),
        ("Devanagari", "नमस्ते दुनिया यह परीक्षण पाठ है"),
    ];

    for (name, text) in test_cases {
        c.bench_with_input(
            BenchmarkId::new("complex_script_render", name),
            &text,
            |b, text| {
                b.iter(|| {
                    let runs = backend.segment(black_box(text), &segment_options).unwrap();
                    for run in &runs {
                        let shaped = backend.shape(black_box(run), black_box(&font)).unwrap();
                        let _ = backend
                            .render(black_box(&shaped), black_box(&render_options))
                            .unwrap();
                    }
                });
            },
        );
    }
}

fn bench_font_sizes(c: &mut Criterion) {
    let backend = get_backend();
    let text = "Test Text";
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    #[cfg(target_os = "macos")]
    let font_name = "Helvetica";
    #[cfg(not(target_os = "macos"))]
    let font_name = "DejaVu Sans";

    let runs = backend.segment(text, &segment_options).unwrap();
    let run = &runs[0];

    let sizes = vec![12.0, 24.0, 48.0, 72.0, 144.0];

    for size in sizes {
        c.bench_with_input(
            BenchmarkId::new("font_size_render", size.to_string()),
            &size,
            |b, &size| {
                let font = Font::new(font_name, size);
                b.iter(|| {
                    let shaped = backend.shape(black_box(run), black_box(&font)).unwrap();
                    let _ = backend
                        .render(black_box(&shaped), black_box(&render_options))
                        .unwrap();
                });
            },
        );
    }
}

fn bench_shape_only(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let text = "The quick brown fox jumps over the lazy dog";
    let segment_options = SegmentOptions::default();

    c.bench_function("shape_only", |b| {
        let runs = backend.segment(text, &segment_options).unwrap();
        let run = &runs[0];
        b.iter(|| {
            let _ = backend.shape(black_box(run), black_box(&font)).unwrap();
        });
    });
}

fn bench_segment_only(c: &mut Criterion) {
    let backend = get_backend();
    let text = "The quick brown fox jumps over the lazy dog. مرحبا بالعالم. 你好世界.";
    let segment_options = SegmentOptions::default();

    c.bench_function("segment_only", |b| {
        b.iter(|| {
            let _ = backend
                .segment(black_box(text), black_box(&segment_options))
                .unwrap();
        });
    });
}

fn bench_render_only(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let text = "Test";
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    let runs = backend.segment(text, &segment_options).unwrap();
    let shaped = backend.shape(&runs[0], &font).unwrap();

    c.bench_function("render_only", |b| {
        b.iter(|| {
            let _ = backend
                .render(black_box(&shaped), black_box(&render_options))
                .unwrap();
        });
    });
}

criterion_group!(
    benches,
    bench_simple_latin,
    bench_complex_scripts,
    bench_font_sizes,
    bench_shape_only,
    bench_segment_only,
    bench_render_only
);
criterion_main!(benches);
