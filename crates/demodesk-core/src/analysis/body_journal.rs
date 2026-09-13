//! Shared, delta-encoded body measurements. No rule or deduction policy lives here.
use super::Artifact;
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, io::BufRead};
#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyClock {
    pub tick_rate: f64,
    pub sample_step_ticks: u32,
    pub angular_resolution_degrees: f64,
    pub measurement_source: String,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BodyTrack {
    pub player_id: String,
    pub round: i32,
    pub target_id: String,
    pub point_id: String,
    pub point_key: String,
    pub enemy: bool,
}
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BodyView {
    pub eye: [f64; 3],
    pub view: [f64; 2],
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Change {
    tick: i32,
    #[serde(default)]
    define: BTreeMap<String, BodyTrack>,
    #[serde(default)]
    views: BTreeMap<String, BodyView>,
    #[serde(default)]
    points: BTreeMap<String, [f64; 3]>,
    active: Option<Vec<String>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct End {
    end: usize,
    last_tick: Option<i32>,
    #[serde(default, rename = "alignment")]
    _alignment: Vec<serde_json::Value>,
}
pub struct BodyFrame<'a> {
    pub tick: i32,
    pub tracks: Vec<&'a BodyTrack>,
    pub views: &'a BTreeMap<String, BodyView>,
    pub points: &'a BTreeMap<String, [f64; 3]>,
}
/// The consumer borrows one reconstructed state; no full-demo snapshots are retained.
pub fn visit(
    mut reader: impl BufRead,
    mut consume: impl FnMut(&Artifact<BodyClock>, BodyFrame<'_>) -> Result<()>,
) -> Result<Artifact<BodyClock>> {
    fn line(reader: &mut impl BufRead) -> Result<Option<Vec<u8>>> {
        let mut bytes = Vec::new();
        std::io::Read::take(reader, 16 * 1024 * 1024 + 1).read_until(b'\n', &mut bytes)?;
        ensure!(
            bytes.len() <= 16 * 1024 * 1024,
            "oversized body journal record"
        );
        Ok((!bytes.is_empty()).then_some(bytes))
    }
    let header: Artifact<BodyClock> =
        serde_json::from_slice(&line(&mut reader)?.context("missing body journal header")?)?;
    ensure!(
        matches!(header.contract.schema_version, 1 | 2),
        "unsupported body journal schema"
    );
    header.require("body-measurement-journal", header.contract.schema_version)?;
    let mut definitions = BTreeMap::new();
    let mut views = BTreeMap::new();
    let mut points = BTreeMap::new();
    let mut active = Vec::new();
    let mut last = None;
    let mut count = 0;
    loop {
        let bytes = line(&mut reader)?.context("missing body journal end marker")?;
        // End records are rare; parse normal typed changes directly.
        let change = match serde_json::from_slice::<Change>(&bytes) {
            Ok(change) => change,
            Err(change_error) => {
                let end: End = serde_json::from_slice(&bytes).map_err(|_| change_error)?;
                ensure!(
                    end.end == count && end.last_tick == last,
                    "body journal end mismatch"
                );
                ensure!(line(&mut reader)?.is_none(), "data after body journal end");
                return Ok(header);
            }
        };
        ensure!(
            change.tick >= 0 && last.is_none_or(|t| change.tick > t),
            "body journal ticks must increase"
        );
        for (key, track) in change.define {
            ensure!(!definitions.contains_key(&key), "body track redefined");
            definitions.insert(key, track);
        }
        views.extend(change.views);
        points.extend(change.points);
        if let Some(ids) = change.active {
            active = ids;
        }
        let mut seen = std::collections::BTreeSet::new();
        let tracks = active
            .iter()
            .map(|id| {
                ensure!(seen.insert(id), "duplicate active body track");
                let track = definitions.get(id).context("undefined body track")?;
                ensure!(
                    views.contains_key(&track.player_id) && points.contains_key(&track.point_key),
                    "missing active body measurement"
                );
                Ok(track)
            })
            .collect::<Result<Vec<_>>>()?;
        consume(
            &header,
            BodyFrame {
                tick: change.tick,
                tracks,
                views: &views,
                points: &points,
            },
        )?;
        last = Some(change.tick);
        count += 1;
    }
}

/// Detect the storage envelope without changing the measurement contract.
pub fn reader(mut file: &std::fs::File) -> Result<Box<dyn BufRead + '_>> {
    use std::io::{Read, Seek};
    file.rewind()?;
    let mut magic = [0u8; 2];
    file.read_exact(&mut magic)?;
    file.rewind()?;
    if magic == [0x1f, 0x8b] {
        Ok(Box::new(std::io::BufReader::new(
            flate2::read::MultiGzDecoder::new(file),
        )))
    } else {
        Ok(Box::new(std::io::BufReader::new(file)))
    }
}
