// this_file: benches/batch_render.rs

//! Batch rendering performance benchmarks

use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use o4e_core::{Backend, Font, RenderOptions, SegmentOptions};
use rayon::prelude::*;

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

fn bench_batch_sizes(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    let test_texts = vec![
        "Hello World",
        "The quick brown fox",
        "Lorem ipsum dolor sit amet",
        "Text rendering benchmark",
        "Multi-backend architecture",
    ];

    let batch_sizes = vec![10, 100, 1000];

    for batch_size in batch_sizes {
        let mut group = c.benchmark_group("batch_render");
        group.throughput(Throughput::Elements(batch_size as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &batch_size,
            |b, &batch_size| {
                let items: Vec<_> = (0..batch_size)
                    .map(|i| test_texts[i % test_texts.len()])
                    .collect();

                b.iter(|| {
                    items.par_iter().for_each(|text| {
                        if let Ok(runs) = backend.segment(black_box(text), &segment_options) {
                            for run in &runs {
                                if let Ok(shaped) = backend.shape(black_box(run), black_box(&font)) {
                                    let _ = backend.render(black_box(&shaped), black_box(&render_options));
                                }
                            }
                        }
                    });
                });
            },
        );
        group.finish();
    }
}

fn bench_parallel_vs_sequential(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    let test_texts: Vec<_> = (0..100)
        .map(|i| format!("Test text number {}", i))
        .collect();

    let mut group = c.benchmark_group("parallel_comparison");
    group.throughput(Throughput::Elements(100));

    group.bench_function("sequential", |b| {
        b.iter(|| {
            for text in &test_texts {
                if let Ok(runs) = backend.segment(black_box(text), &segment_options) {
                    for run in &runs {
                        if let Ok(shaped) = backend.shape(black_box(run), black_box(&font)) {
                            let _ = backend.render(black_box(&shaped), black_box(&render_options));
                        }
                    }
                }
            }
        });
    });

    group.bench_function("parallel", |b| {
        b.iter(|| {
            test_texts.par_iter().for_each(|text| {
                if let Ok(runs) = backend.segment(black_box(text), &segment_options) {
                    for run in &runs {
                        if let Ok(shaped) = backend.shape(black_box(run), black_box(&font)) {
                            let _ = backend.render(black_box(&shaped), black_box(&render_options));
                        }
                    }
                }
            });
        });
    });

    group.finish();
}

fn bench_cache_effectiveness(c: &mut Criterion) {
    let backend = get_backend();
    #[cfg(target_os = "macos")]
    let font = Font::new("Helvetica", 24.0);
    #[cfg(not(target_os = "macos"))]
    let font = Font::new("DejaVu Sans", 24.0);

    let segment_options = SegmentOptions::default();

    // Same text repeated - should benefit from caching
    let text = "Cached text example";

    c.bench_function("cache_first_run", |b| {
        b.iter(|| {
            let runs = backend.segment(black_box(text), &segment_options).unwrap();
            for run in &runs {
                let _ = backend.shape(black_box(run), black_box(&font)).unwrap();
            }
        });
    });

    // Warm up cache
    for _ in 0..10 {
        let runs = backend.segment(text, &segment_options).unwrap();
        for run in &runs {
            let _ = backend.shape(&run, &font).unwrap();
        }
    }

    c.bench_function("cache_warm_run", |b| {
        b.iter(|| {
            let runs = backend.segment(black_box(text), &segment_options).unwrap();
            for run in &runs {
                let _ = backend.shape(black_box(run), black_box(&font)).unwrap();
            }
        });
    });
}

fn bench_mixed_scripts_batch(c: &mut Criterion) {
    let backend = get_backend();
    let font = Font::new("NotoSans", 24.0);
    let segment_options = SegmentOptions::default();
    let render_options = RenderOptions::default();

    let mixed_texts = vec![
        "Hello World",
        "مرحبا بالعالم",
        "你好世界",
        "שלום עולם",
        "Привет мир",
        "Γειά σου κόσμε",
        "こんにちは世界",
        "안녕하세요 세계",
    ];

    let batch_size = mixed_texts.len();
    let mut group = c.benchmark_group("mixed_scripts_batch");
    group.throughput(Throughput::Elements(batch_size as u64));

    group.bench_function("render_all", |b| {
        b.iter(|| {
            mixed_texts.par_iter().for_each(|text| {
                if let Ok(runs) = backend.segment(black_box(text), &segment_options) {
                    for run in &runs {
                        if let Ok(shaped) = backend.shape(black_box(run), black_box(&font)) {
                            let _ = backend.render(black_box(&shaped), black_box(&render_options));
                        }
                    }
                }
            });
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_batch_sizes,
    bench_parallel_vs_sequential,
    bench_cache_effectiveness,
    bench_mixed_scripts_batch
);
criterion_main!(benches);
