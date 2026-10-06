//! `cargo run --example encode_size -- <in.mp4> <out.mp4> <max_mb> <codec>` — exercise encode_to_size.
use demodesk_core::render::encode::{encode_to_size_with_progress, Control, SizeOptions};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ffmpeg = PathBuf::from(std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into()));
    let started = std::time::Instant::now();
    let r = encode_to_size_with_progress(
        &ffmpeg,
        Path::new(&a[1]),
        Path::new(&a[2]),
        SizeOptions {
            max_size_mb: a[3].parse().unwrap(),
            codec: &a[4],
            audio_kbps: 192,
        },
        &mut false,
        &mut |_| {},
        &mut Control::new(&AtomicBool::new(false), &mut |line| {
            println!("[{:.1}s] {line}", started.elapsed().as_secs_f64());
        }),
    )
    .unwrap();
    println!("{} kbps → {} bytes", r.bitrate_kbps, r.bytes);
}
