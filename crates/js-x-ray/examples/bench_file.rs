//! Repeated single-file analysis benchmark. Usage: bench_file <path> [iters]
use js_x_ray::ast_analyser::{AstAnalyser, RuntimeOptions};
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("file path");
    let iters: usize = std::env::args().nth(2).and_then(|a| a.parse().ok()).unwrap_or(30);
    let code = std::fs::read_to_string(&path).expect("readable");
    let analyser = AstAnalyser::default();
    let run = |a: &AstAnalyser| {
        a.analyse(&code, RuntimeOptions { is_minified: true, ..Default::default() })
            .map(|r| r.warnings.len())
            .unwrap_or(0)
    };
    for _ in 0..10 {
        run(&analyser);
    }
    let mut times: Vec<f64> = (0..iters)
        .map(|_| {
            let t0 = Instant::now();
            run(&analyser);
            t0.elapsed().as_secs_f64() * 1000.0
        })
        .collect();
    times.sort_by(|a, b| a.total_cmp(b));
    println!("rust-native {path} median_ms: {:.3}", times[times.len() / 2]);
}
