//! `cargo run --example encode_size -- <in.mp4> <out.mp4> <max_mb> <codec>` — exercise encode_to_size.
use demodesk_core::render::encode::encode_to_size;
use std::path::{Path, PathBuf};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let ffmpeg = PathBuf::from(std::env::var("FFMPEG").unwrap_or_else(|_| "ffmpeg".into()));
    let r = encode_to_size(&ffmpeg, Path::new(&a[1]), Path::new(&a[2]), a[3].parse().unwrap(), &a[4], 192).unwrap();
    println!("{} kbps → {} bytes", r.bitrate_kbps, r.bytes);
}
