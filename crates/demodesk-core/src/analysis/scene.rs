//! Application scene snapshots; packet decoding is owned by the parser module.
pub use crate::parser::{EntitySnapshot, PoseField, SceneFrame};
pub use crate::parser::demoparser::scene::{extract, contract};
