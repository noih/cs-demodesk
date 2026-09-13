//! Shared analysis data. Producers do not depend on scoring rules.
pub mod model_hitboxes;
pub mod event_context;
pub mod animation_pose;
pub mod animation_aim;
pub mod native_body;
pub mod animation_assets;
pub mod animation_clip;
pub mod kv3_text;
pub mod animation_recipe;
pub mod journal;
pub mod compact;
pub mod body_journal;
pub mod measurements;
pub mod scene;
pub mod smoke;

use anyhow::{ensure, Context, Result};
use prost::Message;
use serde::{Deserialize, Serialize};

/// Wire contract, independent of the algorithm that produces the payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract {
    pub module: String,
    pub schema_version: u32,
    pub implementation_version: String,
}

/// Unknown source versions stay unknown; a resource path hash is not a content version.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Source {
    pub demo_fingerprint: Option<String>,
    pub game_build: Option<String>,
    #[serde(default)]
    pub game_patch: Option<String>,
    pub map_content_fingerprint: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact<T> {
    pub contract: Contract,
    pub source: Source,
    /// Exact input artifact fingerprints, not a global ruleset version.
    pub dependencies: Vec<String>,
    pub data: T,
}
impl<T> Artifact<T> {
    pub fn require(&self, module: &str, schema_version: u32) -> Result<&T> {
        ensure!(self.contract.module == module, "unexpected analysis module");
        ensure!(
            self.contract.schema_version == schema_version,
            "unsupported analysis schema"
        );
        ensure!(
            !self.contract.implementation_version.is_empty(),
            "missing implementation version"
        );
        Ok(&self.data)
    }
}

pub fn demo_source(bytes: &[u8]) -> Result<Source> {
    ensure!(
        bytes.len() >= 19 && &bytes[..8] == b"PBDEMS2\0",
        "invalid Source 2 demo header"
    );
    let mut reader = &bytes[16..];
    let mut position = 16;
    let command = crate::demo_readiness::varint(&mut reader, &mut position)?
        .context("invalid header command")?;
    ensure!(command == 1, "expected uncompressed demo file header");
    crate::demo_readiness::varint(&mut reader, &mut position)?.context("invalid header tick")?;
    let size = crate::demo_readiness::varint(&mut reader, &mut position)?
        .context("invalid header size")? as usize;
    let header =
        csgoproto::CDemoFileHeader::decode(reader.get(..size).context("truncated demo header")?)?;
    Ok(Source {
        demo_fingerprint: Some(format!("sha1:{}", sha1_smol::Sha1::from(bytes).digest())),
        game_build: header.build_num.filter(|v| *v > 0).map(|v| v.to_string()),
        game_patch: header
            .patch_version
            .filter(|v| *v > 0)
            .map(|v| v.to_string()),
        map_content_fingerprint: None,
    })
}

/// Require an explicit, consistent source clock; never infer 64 Hz from the game name.
pub fn server_tick_rate(infos: &[csgoproto::CsvcMsgServerInfo]) -> Result<f64> {
    let interval = infos
        .first()
        .and_then(|i| i.tick_interval)
        .context("missing ServerInfo tick interval")?;
    ensure!(
        interval.is_finite() && interval > 0.0,
        "invalid ServerInfo tick interval"
    );
    ensure!(
        infos.iter().all(|i| i.tick_interval == Some(interval)),
        "inconsistent ServerInfo tick intervals"
    );
    Ok(1.0 / f64::from(interval))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_clock_requires_consistent_positive_intervals() {
        let info = |n| csgoproto::CsvcMsgServerInfo {
            tick_interval: n,
            ..Default::default()
        };
        assert_eq!(server_tick_rate(&[info(Some(1.0 / 128.0))]).unwrap(), 128.0);
        for values in [
            vec![],
            vec![info(None)],
            vec![info(Some(0.0))],
            vec![info(Some(f32::NAN))],
            vec![info(Some(1.0 / 64.0)), info(Some(1.0 / 128.0))],
        ] {
            assert!(server_tick_rate(&values).is_err());
        }
    }
    #[test]
    fn source_versions_use_distinct_header_fields_and_reject_truncation() {
        let header = csgoproto::CDemoFileHeader {
            demo_file_stamp: "PBDEMS2".into(),
            patch_version: Some(14181),
            build_num: Some(123),
            ..Default::default()
        }
        .encode_to_vec();
        let mut bytes = b"PBDEMS2\0".to_vec();
        bytes.extend_from_slice(&[0; 8]);
        assert!(header.len() < 128);
        bytes.extend_from_slice(&[1, 0, header.len() as u8]);
        bytes.extend_from_slice(&header);
        let source = demo_source(&bytes).unwrap();
        assert_eq!(source.game_build.as_deref(), Some("123"));
        assert_eq!(source.game_patch.as_deref(), Some("14181"));
        for end in 0..bytes.len() {
            assert!(demo_source(&bytes[..end]).is_err());
        }
        bytes[16] = 65;
        assert!(demo_source(&bytes).is_err());
    }
    #[test]
    fn schema_compatibility_is_independent_of_producer_version() {
        let mut artifact = Artifact {
            contract: scene::contract(),
            source: Source {
                demo_fingerprint: None,
                game_build: None,
                game_patch: None,
                map_content_fingerprint: None,
            },
            dependencies: vec![],
            data: vec![42],
        };
        assert_eq!(artifact.require("packet-scene", 1).unwrap(), &[42]);
        artifact.contract.implementation_version = "2.0.0".into();
        assert!(artifact.require("packet-scene", 1).is_ok());
        assert!(artifact.require("smoke-journal", 1).is_err());
        artifact.contract.schema_version = 2;
        assert!(artifact.require("packet-scene", 1).is_err());
        assert!(artifact.source.game_build.is_none());
    }
}
