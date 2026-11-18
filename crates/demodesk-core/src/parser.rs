//! Demo reading boundary. Consumers depend on app-owned results; the default adapter is replaceable.
mod contract;
pub(crate) mod demoparser;
mod normalize;
mod rows;
use crate::{replay::ReplayEvent, smoke::projection::Snapshot};
use anyhow::Result;
pub use contract::*;
pub(crate) use normalize::Fields;
pub use rows::{Column, Row, Rows};
use std::{collections::HashMap, io::Write};

/// Metadata, events and samples needed by match analysis and editing.
pub trait DemoQueries: Send + Sync {
    fn header(&self, bytes: &[u8]) -> Result<HashMap<String, String>>;
    fn server_info(&self, bytes: &[u8]) -> Result<Vec<ServerInfo>>;
    fn property_names(&self, bytes: &[u8]) -> Result<Vec<String>>;
    fn events(
        &self,
        bytes: &[u8],
        names: &[String],
        player_extra: &[String],
        other_extra: &[String],
    ) -> Result<EventBatch>;
    fn ticks(&self, bytes: &[u8], props: &[String], ticks: Vec<i32>) -> Result<Rows>;
    fn projectiles(&self, bytes: &[u8], ticks: Vec<i32>) -> Result<Rows>;
}
/// Packet streaming for pose, smoke and lossless analysis; separate from ordinary queries.
pub trait DemoStream: Send + Sync {
    fn visit_scene_frames(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        step: i32,
        tracking_only: bool,
        visit: &mut dyn FnMut(SceneFrame) -> Result<()>,
    ) -> Result<()>;
    fn write_match_state(&self, bytes: &[u8], output: &mut dyn Write) -> Result<()>;
    fn smoke_replay(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        rate: f64,
        events: &[ReplayEvent],
    ) -> Result<Vec<Snapshot>>;
}
pub trait DemoBackend: DemoQueries + DemoStream {}
impl<T: DemoQueries + DemoStream> DemoBackend for T {}

pub struct DemoParser {
    backend: Box<dyn DemoBackend>,
}
impl Default for DemoParser {
    fn default() -> Self {
        Self::new()
    }
}
impl DemoParser {
    pub fn new() -> Self {
        Self::with_backend(demoparser::DemoparserBackend::new())
    }
    pub fn with_backend(backend: impl DemoBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }
    pub fn header(&self, bytes: &[u8]) -> Result<HashMap<String, String>> {
        self.backend.header(bytes)
    }
    pub fn server_info(&self, bytes: &[u8]) -> Result<Vec<ServerInfo>> {
        self.backend.server_info(bytes)
    }
    pub fn property_names(&self, bytes: &[u8]) -> Result<Vec<String>> {
        self.backend.property_names(bytes)
    }
    pub fn events(
        &self,
        bytes: &[u8],
        names: &[String],
        player_extra: &[String],
        other_extra: &[String],
    ) -> Result<EventBatch> {
        self.backend.events(bytes, names, player_extra, other_extra)
    }
    pub fn ticks(&self, bytes: &[u8], props: &[String], ticks: Vec<i32>) -> Result<Rows> {
        self.backend.ticks(bytes, props, ticks)
    }
    pub fn projectiles(&self, bytes: &[u8], ticks: Vec<i32>) -> Result<Rows> {
        self.backend.projectiles(bytes, ticks)
    }
    pub fn smoke_replay(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        rate: f64,
        events: &[ReplayEvent],
    ) -> Result<Vec<Snapshot>> {
        self.backend.smoke_replay(bytes, first, last, rate, events)
    }
    pub fn write_match_state(&self, bytes: &[u8], mut output: impl Write) -> Result<()> {
        self.backend.write_match_state(bytes, &mut output)
    }
    pub fn visit_scene_frames(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        step: i32,
        tracking_only: bool,
        mut visit: impl FnMut(SceneFrame) -> Result<()>,
    ) -> Result<()> {
        self.backend
            .visit_scene_frames(bytes, first, last, step, tracking_only, &mut visit)
    }
    pub fn scene_frames(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        step: i32,
    ) -> Result<Vec<SceneFrame>> {
        self.collect_scene_frames(bytes, first, last, step, false)
    }
    pub fn player_tracking_frames(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        step: i32,
    ) -> Result<Vec<SceneFrame>> {
        self.collect_scene_frames(bytes, first, last, step, true)
    }
    fn collect_scene_frames(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        step: i32,
        tracking_only: bool,
    ) -> Result<Vec<SceneFrame>> {
        let mut frames = Vec::new();
        self.visit_scene_frames(bytes, first, last, step, tracking_only, |frame| {
            frames.push(frame);
            Ok(())
        })?;
        Ok(frames)
    }
}

pub(crate) fn replicated_convars(bytes: &[u8], names: &[&str]) -> Result<ReplicatedConvars> {
    demoparser::replicated_convars(bytes, names)
}

#[cfg(test)]
mod tests;
