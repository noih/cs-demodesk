//! demodesk-core — CS DemoDesk core: CS2 demo parsing, highlight detection, statistics and the
//! HLAE render pipeline. No Tauri dependency so it can be unit-tested anywhere.

pub mod detector;
pub mod engine;
pub mod radar;
pub mod model;
pub mod parser;
pub mod render;
pub mod replay;
pub mod stats;
pub mod store;

pub use model::*;

mod demo_readiness;
