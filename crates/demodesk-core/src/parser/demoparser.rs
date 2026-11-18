//! demoparser adapter. Upstream types and wire-format changes stay in this module.
use super::Rows;
use crate::error::ErrorSource;
use ahash::AHashMap;
use anyhow::{anyhow, Result};
use parser::first_pass::parser_settings::{rm_user_friendly_names, FirstPassParser, ParserInputs};
use parser::parse_demo::{Parser, ParsingMode};
use parser::second_pass::parser_settings::create_huffman_lookup_table;
use std::collections::HashMap;
mod convert;
pub(crate) use convert::replicated_convars;
use convert::{convert_event, convert_events, convert_rows};
pub(crate) mod compact;
pub(crate) mod scene;
pub(crate) mod smoke;
/// Reusable parser context: the huffman table is expensive to build, so build it once per process.
pub struct DemoparserBackend {
    huffman: Vec<(u8, u8)>,
}

impl Default for DemoparserBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl DemoparserBackend {
    pub fn new() -> Self {
        Self {
            huffman: create_huffman_lookup_table(),
        }
    }

    pub(crate) fn inputs<'a>(
        &'a self,
        player_props: &[String],
        other_props: &[String],
        events: &[String],
        ticks: Vec<i32>,
    ) -> Result<ParserInputs<'a>> {
        self.inputs_ex(player_props, other_props, events, ticks, false)
    }

    fn inputs_ex<'a>(
        &'a self,
        player_props: &[String],
        other_props: &[String],
        events: &[String],
        ticks: Vec<i32>,
        projectiles: bool,
    ) -> Result<ParserInputs<'a>> {
        let real_player = rm_user_friendly_names(&player_props.to_vec())
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("{e}")))?;
        let real_other = rm_user_friendly_names(&other_props.to_vec())
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("{e}")))?;
        let mut real_name_to_og_name = AHashMap::default();
        for (real, friendly) in real_player.iter().zip(player_props) {
            real_name_to_og_name.insert(real.clone(), friendly.clone());
        }
        for (real, friendly) in real_other.iter().zip(other_props) {
            real_name_to_og_name.insert(real.clone(), friendly.clone());
        }
        Ok(ParserInputs {
            real_name_to_og_name,
            wanted_players: vec![],
            wanted_player_props: real_player,
            wanted_other_props: real_other,
            wanted_prop_states: AHashMap::default(),
            wanted_ticks: ticks,
            wanted_events: events.to_vec(),
            parse_ents: true,
            parse_projectiles: projectiles,
            parse_grenades: projectiles,
            only_header: false,
            only_convars: false,
            huffman_lookup_table: &self.huffman,
            order_by_steamid: false,
            list_props: false,
            fallback_bytes: None,
        })
    }
}

impl super::DemoQueries for DemoparserBackend {
    fn header(&self, bytes: &[u8]) -> Result<HashMap<String, String>> {
        let inputs = self.inputs(&[], &[], &[], vec![])?;
        let mut first = FirstPassParser::new(&inputs);
        let header = first
            .parse_header_only(bytes)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("header: {e}")))?;
        Ok(header.into_iter().collect())
    }
    fn server_info(&self, bytes: &[u8]) -> Result<Vec<super::ServerInfo>> {
        let inputs = self.inputs(&[], &[], &[], vec![])?;
        let mut first = FirstPassParser::new(&inputs);
        Ok(first
            .parse_demo(bytes, true)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("server info: {e}")))?
            .server_infos
            .into_iter()
            .map(Into::into)
            .collect())
    }
    fn property_names(&self, bytes: &[u8]) -> Result<Vec<String>> {
        let mut inputs = self.inputs(&[], &[], &[], vec![])?;
        inputs.list_props = true;
        let mut first = FirstPassParser::new(&inputs);
        let out = first
            .parse_demo(bytes, true)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("properties: {e}")))?;
        let mut names: Vec<_> = out.prop_controller.name_to_id.keys().cloned().collect();
        names.sort();
        Ok(names)
    }
    fn events(
        &self,
        bytes: &[u8],
        names: &[String],
        player_extra: &[String],
        other_extra: &[String],
    ) -> Result<super::EventBatch> {
        let inputs = self.inputs(player_extra, other_extra, names, vec![])?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        parser
            .parse_demo(bytes)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("events: {e}")))
            .map(convert_events)
    }
    fn ticks(&self, bytes: &[u8], props: &[String], wanted_ticks: Vec<i32>) -> Result<Rows> {
        let inputs = self.inputs(props, &[], &[], wanted_ticks)?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        let out = parser
            .parse_demo(bytes)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("ticks: {e}")))?;
        Ok(convert_rows(out))
    }
    fn projectiles(&self, bytes: &[u8], wanted_ticks: Vec<i32>) -> Result<Rows> {
        let inputs = self.inputs_ex(
            &[
                "m_firePositions".into(),
                "m_fireCount".into(),
                "m_nInfernoType".into(),
                "m_nVoxelFrameDataSize".into(),
            ],
            &[],
            &[],
            wanted_ticks,
            true,
        )?;
        let mut parser = Parser::new(inputs, ParsingMode::Normal);
        let out = parser
            .parse_demo(bytes)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("projectiles: {e}")))?;
        Ok(convert_rows(out))
    }
}
impl super::DemoStream for DemoparserBackend {
    fn visit_scene_frames(
        &self,
        bytes: &[u8],
        first_tick: i32,
        last_tick: i32,
        step: i32,
        tracking_only: bool,
        visit: &mut dyn FnMut(crate::analysis::scene::SceneFrame) -> Result<()>,
    ) -> Result<()> {
        anyhow::ensure!(
            first_tick >= 0 && last_tick >= first_tick && step > 0,
            "invalid scene interval"
        );
        let inputs = self.inputs(&[], &[], &[], vec![i32::MIN])?;
        let mut first = FirstPassParser::new(&inputs);
        let output = first
            .parse_demo(bytes, true)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("scene schema: {e}")))?;
        let mut second = parser::second_pass::parser_settings::SecondPassParser::new(
            output,
            parser::first_pass::parser::HEADER_ENDS_AT_BYTE,
            true,
            None,
        )
        .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("scene parser: {e}")))?;
        second.capture_pose_fields = !tracking_only;
        let mut pending: Option<crate::analysis::scene::SceneFrame> = None;
        let mut failure = None;
        second
            .start_with_observer(bytes, |p| {
                if pending.as_ref().is_some_and(|frame| p.tick > frame.tick) {
                    if let Err(error) = visit(pending.take().unwrap()) {
                        failure = Some(error);
                        return false;
                    }
                }
                if p.tick > last_tick {
                    return false;
                }
                if p.tick >= first_tick && (p.tick - first_tick) % step == 0 {
                    pending = Some(scene::capture(p, tracking_only));
                }
                true
            })
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("scene packets: {e}")))?;
        if let Some(error) = failure {
            return Err(error);
        }
        if let Some(frame) = pending {
            visit(frame)?;
        }
        Ok(())
    }
    fn write_match_state(&self, bytes: &[u8], output: &mut dyn std::io::Write) -> Result<()> {
        let inputs = self.inputs(&[], &[], &["all".into()], vec![i32::MIN])?;
        let mut first = FirstPassParser::new(&inputs);
        let parsed = first
            .parse_demo(bytes, true)
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("analysis schema: {e}")))?;
        let tick_rate = crate::analysis::server_tick_rate(
            &parsed
                .server_infos
                .iter()
                .map(|i| super::ServerInfo {
                    tick_interval: i.tick_interval,
                    ..Default::default()
                })
                .collect::<Vec<_>>(),
        )?;
        let mut writer =
            compact::Writer::new(output, crate::analysis::demo_source(bytes)?, tick_rate)?;
        let mut second = parser::second_pass::parser_settings::SecondPassParser::new(
            parsed,
            parser::first_pass::parser::HEADER_ENDS_AT_BYTE,
            true,
            None,
        )
        .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("analysis parser: {e}")))?;
        second.capture_pose_fields = true;
        second.analysis_changes = Some(Default::default());
        let mut failure = None;
        second
            .start_with_observer(bytes, |p| {
                if let Err(error) = writer.push(p) {
                    failure = Some(error);
                    return false;
                }
                true
            })
            .map_err(|e| ErrorSource::DemoParser.wrap(anyhow!("analysis packets: {e}")))?;
        if let Some(error) = failure {
            return Err(error);
        }
        let mut events: Vec<_> = second.game_events.into_iter().map(convert_event).collect();
        // An incomplete or unsupported source keeps policy absent, never guessed.
        if let Ok(policies) = crate::analysis::smoke::spread::policies_for_demo(bytes) {
            super::normalize::annotate_smoke_policy(&mut events, policies);
        }
        writer.events(&events)?;
        writer.finish()?;
        Ok(())
    }
    fn smoke_replay(
        &self,
        bytes: &[u8],
        first: i32,
        last: i32,
        rate: f64,
        events: &[crate::replay::ReplayEvent],
    ) -> Result<Vec<crate::smoke::projection::Snapshot>> {
        smoke::replay(self, bytes, first, last, rate, events)
    }
}
