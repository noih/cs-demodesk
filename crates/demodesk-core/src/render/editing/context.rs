//! Data acquisition only. Detectors receive shared facts, never file or game handles.
use super::*;
use crate::analysis::visibility_assets;
use anyhow::Context as _;

pub(super) struct Spatial {
    pub tracks: demo_data::Tracks,
    pub geometry: visibility_assets::Prepared,
}
pub(super) struct Context<'a> {
    pub demo: &'a DemoInfo,
    pub originals: &'a [Highlight],
    pub recording: &'a [Highlight],
    pub events: Vec<demo_data::Event>,
    pub spatial: Option<Spatial>,
    pub cancel: &'a AtomicBool,
}
impl<'a> Context<'a> {
    pub fn load(
        demo: &'a DemoInfo,
        originals: &'a [Highlight],
        recording: &'a [Highlight],
        tools: &ToolPaths,
        root: &Path,
        spatial: bool,
        cancel: &'a AtomicBool,
    ) -> Result<Self> {
        let path = Path::new(&demo.path);
        Control::new(cancel, &mut |_| {}).check_cancelled()?;
        let bytes = std::fs::read(path)?;
        let events = demo_data::read_events(&bytes, cancel)?;
        let spatial = spatial
            .then(|| -> Result<Spatial> {
                ensure!(demo.tick_rate >= 8., "invalid spatial sampling rate");
                let game = tools
                    .cs2_dir
                    .as_deref()
                    .context("Map directory unavailable")?;
                let vrf = tools
                    .vrf_exe
                    .as_deref()
                    .context("Map extractor unavailable")?;
                let tracks = demo_data::read_tracks(
                    &bytes,
                    recording,
                    demo.tick_rate.round() as i32,
                    cancel,
                )?;
                drop(bytes);
                let geometry = visibility_assets::prepare(root, game, vrf, &demo.map_name)?;
                Ok(Spatial { tracks, geometry })
            })
            .and_then(|result| {
                result
                    .map_err(|error| eprintln!("Spatial analysis unavailable: {error:#}"))
                    .ok()
            });
        Control::new(cancel, &mut |_| {}).check_cancelled()?;
        Ok(Self {
            demo,
            originals,
            recording,
            events,
            spatial,
            cancel,
        })
    }
    pub fn clips(&self, id: &str) -> Vec<Highlight> {
        self.recording
            .iter()
            .filter(|h| h.id == id)
            .cloned()
            .collect()
    }
}

/// Selection/compilation deliberately has no raw events, parser, filesystem or tools.
pub(super) struct Composition<'a> {
    pub demo: &'a DemoInfo,
    pub originals: &'a [Highlight],
}
