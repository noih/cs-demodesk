//! Parse a demo and print the highlights / stats; optionally dump the JSON the
//! app would store:  cargo run --release --example parse -- <demo.dem> [out.json] [kills.json]
use demodesk_core::parser::DemoParser;
use demodesk_core::stats::build_parsed_demo;
use std::path::Path;

fn main() {
    let mut args = std::env::args().skip(1);
    let path = args.next().expect("demo path");
    let t = std::time::Instant::now();
    let parsed = build_parsed_demo(DemoParser::new().load_demo(Path::new(&path)).expect("parse"));
    println!("{} kills={} rounds={} highlights={} score={:?} in {:.1?}", parsed.info.map_name, parsed.kills.len(), parsed.rounds.len(), parsed.highlights.len(), parsed.score, t.elapsed());
    for h in parsed.highlights.iter().take(12) {
        println!("  {:5.1}  {:<40} {:?}", h.score, h.title, h.tags);
    }
    for s in &parsed.stats {
        println!("  {:?} {:<16} K{} D{} A{} HS{}% {:?} clutch={}", s.team, s.name, s.kills, s.deaths, s.assists, s.headshot_pct, s.multi_kills, s.clutches_won);
    }
    if let Some(out) = args.next() {
        std::fs::write(&out, serde_json::to_string(&parsed.without_kills()).unwrap()).unwrap();
        println!("wrote {out}");
    }
    if let Some(out) = args.next() {
        std::fs::write(&out, serde_json::to_string(&parsed.kills).unwrap()).unwrap();
        println!("wrote {out}");
    }
}
