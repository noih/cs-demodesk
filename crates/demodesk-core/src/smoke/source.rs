//! Smoke replay uses the configured parser backend.
pub fn replay(parser: &crate::parser::DemoParser, bytes: &[u8], first: i32, last: i32,
    rate: f64, events: &[crate::replay::ReplayEvent]) -> anyhow::Result<Vec<super::projection::Snapshot>> {
    parser.smoke_replay(bytes, first, last, rate, events)
}
