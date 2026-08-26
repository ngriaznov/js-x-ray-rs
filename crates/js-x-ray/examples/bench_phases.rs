//! Phase timing over one file: oxc parse, ESTree JSON serialize, serde parse,
//! loc injection, full analyse. Usage: bench_phases <path>
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("file path");
    let code = std::fs::read_to_string(&path).expect("readable");

    let t0 = Instant::now();
    let allocator = oxc_allocator::Allocator::default();
    let ret = oxc_parser::Parser::new(&allocator, &code, oxc_span::SourceType::mjs()).parse();
    println!(
        "oxc parse:        {:.1} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );

    let t0 = Instant::now();
    let json = ret.program.to_estree_json(false, false);
    println!(
        "estree serialize: {:.1} ms ({} MB JSON)",
        t0.elapsed().as_secs_f64() * 1000.0,
        json.len() / 1_000_000
    );

    let t0 = Instant::now();
    let value: serde_json::Value = serde_json::from_str(&json).expect("valid");
    println!(
        "serde parse:      {:.1} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );
    drop(value);

    let t0 = Instant::now();
    let body = <js_x_ray_rs::JsSourceParser as js_x_ray_rs::SourceParser>::parse(
        &js_x_ray_rs::JsSourceParser,
        &code,
    )
    .expect("parses");
    println!(
        "full parse+loc:   {:.1} ms",
        t0.elapsed().as_secs_f64() * 1000.0
    );
    drop(body);

    let t0 = Instant::now();
    let analyser = js_x_ray_rs::AstAnalyser::default();
    let report = analyser
        .analyse(
            &code,
            js_x_ray_rs::RuntimeOptions {
                is_minified: true,
                ..Default::default()
            },
        )
        .expect("analyses");
    println!(
        "full analyse:     {:.1} ms ({} warnings)",
        t0.elapsed().as_secs_f64() * 1000.0,
        report.warnings.len()
    );
}
