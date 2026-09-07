//! Build the replay stream for a demo and print size / timing:
//!   cargo run --release --example replay -- <demo.dem> [out.json]
use demodesk_core::parser::DemoParser;
use demodesk_core::replay::build_replay;
use std::path::Path;
use std::time::Instant;

fn main() {
    let path = std::env::args().nth(1).expect("demo path");
    let out = std::env::args().nth(2);
    let parser = DemoParser::new();
    let bytes = std::fs::read(&path).expect("read demo");
    let t = Instant::now();
    let demo = parser.load_demo_bytes(Path::new(&path), &bytes).expect("parse");
    println!("parse: {:.1}s, {} rounds, {} players", t.elapsed().as_secs_f64(), demo.rounds.len(), demo.info.players.len());
    let t = Instant::now();
    let replay = build_replay(&parser, &demo.info, &demo.rounds, &bytes).expect("replay");
    let json = serde_json::to_vec(&replay).unwrap();
    println!("replay: {:.1}s, ticks {}..{}, {} frames, {} grenade rows, {} events, {} weapons, {:.1} MB json", t.elapsed().as_secs_f64(), replay.first_tick, replay.last_tick, replay.frames.len(), replay.frames.iter().map(|f| f.g.len()).sum::<usize>(), replay.events.len(), replay.weapons.len(), json.len() as f64 / 1_048_576.0);
    let mut kinds = std::collections::BTreeMap::new();
    for e in &replay.events {
        *kinds.entry(e.k.clone()).or_insert(0) += 1;
    }
    println!("events: {kinds:?}");
    println!("weapons: {:?}", replay.weapons);
    if let Some(f) = replay.frames.iter().find(|f| f.p.len() >= 10) {
        println!("sample frame t={}: {:?}", f.t, &f.p[..3]);
    }
    if let Some(out) = out {
        std::fs::write(&out, &json).unwrap();
        println!("wrote {out}");
    }
}
