//! Lossless match-state contract; its wire adapter is owned by the parser module.
pub use crate::parser::demoparser::compact::{
    contract, retains_field, value, visit, pose_bytes, complete_pose_bytes, pose_array,
    memory_limit, prepare, Metadata, Field, Frame, Summary,
};
