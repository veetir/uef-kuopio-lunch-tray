use criterion::{black_box, criterion_group, criterion_main, Criterion};
use lunch_tray::bench_support::{
    bench_build_line_count, bench_favorite_match_range_count, bench_snapshot_clone,
    bench_split_component_suffix, parse_provider_fixture, provider_fixtures, sample_app_state,
};

fn bench_parse_cached_payload(c: &mut Criterion) {
    let mut group = c.benchmark_group("parse_cached_payload");
    for fixture in provider_fixtures() {
        group.bench_function(fixture.name, |b| {
            b.iter(|| black_box(parse_provider_fixture(black_box(&fixture))))
        });
    }
    group.finish();
}

fn bench_build_lines(c: &mut Criterion) {
    let state = sample_app_state();
    c.bench_function("build_lines/full_state", |b| {
        b.iter(|| black_box(bench_build_line_count(black_box(&state))))
    });
}

fn bench_split_component_suffix_path(c: &mut Criterion) {
    let inputs = [
        "Mausteisia vonertaytteita (*, A, ILM, L, M, Veg, VS)",
        "Pikkelikasviksia pyydettaessa G",
        "BBQ maustettua broileripastaa ja soijapapuja (*, A, L, M, VS)",
        "Tomaattikeittoa ja valimerellista juustoa (A, G, L, VS)",
    ];
    c.bench_function("split_component_suffix/mixed_tokens", |b| {
        b.iter(|| {
            let total: usize = inputs
                .iter()
                .map(|input| bench_split_component_suffix(black_box(input)))
                .sum();
            black_box(total)
        })
    });
}

fn bench_favorite_match_ranges(c: &mut Criterion) {
    let text = "BBQ maustettua broileripastaa ja soijapapuja tomaattikastikkeessa";
    let snippets = vec![
        "broileri".to_string(),
        "soijapapu".to_string(),
        "tomaatti".to_string(),
        "broileripasta".to_string(),
        "kastike".to_string(),
    ];
    c.bench_function("favorite_match_ranges/overlapping_snippets", |b| {
        b.iter(|| {
            black_box(bench_favorite_match_range_count(
                black_box(text),
                black_box(&snippets),
            ))
        })
    });
}

fn bench_app_state_clone(c: &mut Criterion) {
    let state = sample_app_state();
    c.bench_function("snapshot_clone/full_state", |b| {
        b.iter(|| black_box(bench_snapshot_clone(black_box(&state))))
    });
}

criterion_group!(
    benches,
    bench_parse_cached_payload,
    bench_build_lines,
    bench_split_component_suffix_path,
    bench_favorite_match_ranges,
    bench_app_state_clone
);
criterion_main!(benches);
