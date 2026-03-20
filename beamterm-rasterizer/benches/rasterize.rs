use beamterm_data::FontStyle;
use beamterm_rasterizer::NativeRasterizer;
use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};

fn make_rasterizer() -> NativeRasterizer {
    for families in [
        &["Hack"][..],
        &["DejaVu Sans Mono"],
        &["Liberation Mono"],
        &["Noto Sans Mono"],
        &["Courier New"],
        &["monospace"],
    ] {
        if let Ok(r) = NativeRasterizer::new(families, 16.0) {
            return r;
        }
    }
    panic!("no monospace font found");
}

fn bench_rasterize_single(c: &mut Criterion) {
    let mut rasterizer = make_rasterizer();

    let glyphs: &[(&str, &str)] = &[
        ("A", "ascii"),
        ("@", "ascii_complex"),
        ("█", "full_block"),
        ("║", "box_drawing"),
    ];

    let mut group = c.benchmark_group("rasterize_single");
    for &(grapheme, label) in glyphs {
        group.bench_with_input(BenchmarkId::new("normal", label), grapheme, |b, g| {
            b.iter(|| rasterizer.rasterize(g, FontStyle::Normal).unwrap());
        });
    }

    // bold + italic variants for ASCII
    group.bench_function("bold/A", |b| {
        b.iter(|| rasterizer.rasterize("A", FontStyle::Bold).unwrap());
    });
    group.bench_function("italic/A", |b| {
        b.iter(|| rasterizer.rasterize("A", FontStyle::Italic).unwrap());
    });

    group.finish();
}

fn bench_rasterize_ascii_burst(c: &mut Criterion) {
    let mut rasterizer = make_rasterizer();

    // printable ASCII range (space through tilde)
    let ascii_chars: Vec<String> = (0x20u8..=0x7E).map(|b| String::from(b as char)).collect();

    c.benchmark_group("rasterize_burst")
        .bench_function("ascii_95", |b| {
            b.iter(|| {
                for ch in &ascii_chars {
                    let _ = rasterizer.rasterize(ch, FontStyle::Normal).unwrap();
                }
            });
        });
}

fn bench_rasterize_double_width(c: &mut Criterion) {
    let mut rasterizer = make_rasterizer();

    let wide_glyphs: &[(&str, &str)] = &[
        ("\u{4E2D}", "cjk_中"),
        ("\u{1F680}", "emoji_rocket"),
    ];

    let mut group = c.benchmark_group("rasterize_double_width");
    for &(grapheme, label) in wide_glyphs {
        group.bench_with_input(BenchmarkId::new("normal", label), grapheme, |b, g| {
            b.iter(|| rasterizer.rasterize(g, FontStyle::Normal).unwrap());
        });
    }
    group.finish();
}

fn bench_is_double_width(c: &mut Criterion) {
    let mut rasterizer = make_rasterizer();

    let glyphs: &[(&str, &str)] = &[
        ("A", "ascii"),
        ("\u{4E2D}", "cjk"),
        ("\u{1F680}", "emoji"),
        ("\u{E0B0}", "powerline"),
    ];

    let mut group = c.benchmark_group("is_double_width");
    for &(grapheme, label) in glyphs {
        group.bench_with_input(BenchmarkId::new("check", label), grapheme, |b, g| {
            b.iter(|| rasterizer.is_double_width(g));
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_rasterize_single,
    bench_rasterize_ascii_burst,
    bench_rasterize_double_width,
    bench_is_double_width,
);
criterion_main!(benches);
