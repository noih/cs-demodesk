use super::super::{self as contract, Column, Rows};
use parser::{
    parse_demo::DemoOutput,
    second_pass::{
        game_events::GameEvent,
        variants::{VarVec, Variant},
    },
};
impl From<Variant> for contract::Variant {
    fn from(value: Variant) -> Self {
        match value {
            Variant::Bool(v) => Self::Bool(v),
            Variant::U32(v) => Self::U32(v),
            Variant::I32(v) => Self::I32(v),
            Variant::F32(v) => Self::F32(v),
            Variant::U64(v) => Self::U64(v),
            Variant::String(v) => Self::String(v),
            Variant::VecXY(v) => Self::VecXY(v),
            Variant::VecXYZ(v) => Self::VecXYZ(v),
            Variant::StringVec(v) => Self::StringVec(v),
            Variant::U32Vec(v) => Self::U32Vec(v),
            Variant::U64Vec(v) => Self::U64Vec(v),
            Variant::Stickers(v) => Self::Stickers(
                v.into_iter()
                    .map(|v| contract::Sticker {
                        name: v.name,
                        wear: v.wear,
                        id: v.id,
                        x: v.x,
                        y: v.y,
                    })
                    .collect(),
            ),
            Variant::InputHistory(v) => Self::InputHistory(
                v.into_iter()
                    .map(|v| contract::InputHistory {
                        x: v.x,
                        y: v.y,
                        z: v.z,
                        render_tick_count: v.render_tick_count,
                        render_tick_fraction: v.render_tick_fraction,
                        player_tick_count: v.player_tick_count,
                        player_tick_fraction: v.player_tick_fraction,
                    })
                    .collect(),
            ),
            Variant::UserCmdSubtickMoves(v) => Self::UserCmdSubtickMoves(
                v.into_iter()
                    .map(|v| contract::UserCmdSubtickMove {
                        when: v.when,
                        button: v.button,
                        pressed: v.pressed,
                        analog_forward: v.analog_forward,
                        analog_left: v.analog_left,
                        pitch_delta: v.pitch_delta,
                        yaw_delta: v.yaw_delta,
                    })
                    .collect(),
            ),
        }
    }
}
pub(super) fn convert_event(event: GameEvent) -> contract::GameEvent {
    contract::GameEvent {
        name: event.name,
        tick: event.tick,
        fields: event
            .fields
            .into_iter()
            .map(|f| contract::EventField {
                name: f.name,
                data: f.data.map(Into::into),
            })
            .collect(),
    }
}
pub(super) fn convert_events(out: DemoOutput) -> contract::EventBatch {
    contract::EventBatch {
        game_events: out.game_events.into_iter().map(convert_event).collect(),
        server_infos: out.server_infos.into_iter().map(Into::into).collect(),
        players: (if out.player_md.is_empty() {
            out.roster
        } else {
            out.player_md
        })
        .into_iter()
        .map(|p| contract::PlayerMetadata {
            name: p.name,
            steamid: p.steamid,
            team_number: p.team_number,
        })
        .collect(),
    }
}
pub(super) fn convert_rows(mut out: DemoOutput) -> Rows {
    let mut columns = Vec::new();
    let mut len = 0;
    for info in &out.prop_controller.prop_infos {
        let Some(col) = out.df.remove(&info.id) else {
            continue;
        };
        let Some(data) = col.data else {
            len = len.max(col.num_nones);
            continue;
        };
        let data = match data {
            VarVec::Bool(v) => Column::Bool(v),
            VarVec::U32(v) => Column::U32(v),
            VarVec::I32(v) => Column::I32(v),
            VarVec::F32(v) => Column::F32(v),
            VarVec::U64(v) => Column::U64(v),
            VarVec::String(v) => Column::String(v),
            VarVec::StringVec(v) => Column::StringVec(v),
            VarVec::XYZVec(v) => Column::XYZVec(v),
            _ => continue,
        };
        len = len.max(super::super::rows::col_len(&data));
        columns.push((info.prop_friendly_name.clone(), data));
    }
    Rows::from_columns(columns, len)
}

pub(crate) fn replicated_convars(
    bytes: &[u8],
    names: &[&str],
) -> anyhow::Result<contract::ReplicatedConvars> {
    let source = parser::first_pass::convars::read(bytes, names)
        .map_err(|e| crate::error::ErrorSource::DemoParser.wrap(anyhow::anyhow!("convars: {e}")))?;
    Ok(contract::ReplicatedConvars {
        patch_version: source.header().patch_version,
        demo_version_name: source.header().demo_version_name.clone(),
        fires: source
            .fires()
            .iter()
            .map(|f| contract::FireConvars {
                tick: f.tick,
                player: f.player,
                seed: f.seed,
                item_def_index: f.item_def_index,
                values: f.values.clone(),
            })
            .collect(),
    })
}
impl From<contract::Variant> for Variant {
    fn from(value: contract::Variant) -> Self {
        match value {
            contract::Variant::Bool(v) => Self::Bool(v),
            contract::Variant::U32(v) => Self::U32(v),
            contract::Variant::I32(v) => Self::I32(v),
            contract::Variant::F32(v) => Self::F32(v),
            contract::Variant::U64(v) => Self::U64(v),
            contract::Variant::String(v) => Self::String(v),
            contract::Variant::VecXY(v) => Self::VecXY(v),
            contract::Variant::VecXYZ(v) => Self::VecXYZ(v),
            contract::Variant::StringVec(v) => Self::StringVec(v),
            contract::Variant::U32Vec(v) => Self::U32Vec(v),
            contract::Variant::U64Vec(v) => Self::U64Vec(v),
            contract::Variant::Stickers(v) => Self::Stickers(
                v.into_iter()
                    .map(|v| parser::second_pass::variants::Sticker {
                        name: v.name,
                        wear: v.wear,
                        id: v.id,
                        x: v.x,
                        y: v.y,
                    })
                    .collect(),
            ),
            contract::Variant::InputHistory(v) => Self::InputHistory(
                v.into_iter()
                    .map(|v| parser::second_pass::variants::InputHistory {
                        x: v.x,
                        y: v.y,
                        z: v.z,
                        render_tick_count: v.render_tick_count,
                        render_tick_fraction: v.render_tick_fraction,
                        player_tick_count: v.player_tick_count,
                        player_tick_fraction: v.player_tick_fraction,
                    })
                    .collect(),
            ),
            contract::Variant::UserCmdSubtickMoves(v) => Self::UserCmdSubtickMoves(
                v.into_iter()
                    .map(|v| parser::second_pass::variants::UserCmdSubtickMove {
                        when: v.when,
                        button: v.button,
                        pressed: v.pressed,
                        analog_forward: v.analog_forward,
                        analog_left: v.analog_left,
                        pitch_delta: v.pitch_delta,
                        yaw_delta: v.yaw_delta,
                    })
                    .collect(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn application_values_preserve_wire_bits_and_serialization() {
        for wire in [
            Variant::U64(u64::MAX),
            Variant::I32(-1),
            Variant::F32(-0.0),
            Variant::VecXYZ([1.25, -2.5, f32::from_bits(1)]),
            Variant::U32Vec(vec![0, 128, 255]),
        ] {
            let app: contract::Variant = wire.clone().into();
            let restored: Variant = app.clone().into();
            assert_eq!(
                serde_json::to_value(&wire).unwrap(),
                serde_json::to_value(&app).unwrap()
            );
            assert_eq!(
                super::super::compact::wire_value(&wire).unwrap(),
                super::super::compact::wire_value(&restored).unwrap()
            );
        }
    }
}

impl From<csgoproto::CsvcMsgServerInfo> for contract::ServerInfo {
    fn from(info: csgoproto::CsvcMsgServerInfo) -> Self {
        Self {
            tick_interval: info.tick_interval,
            protocol: info.protocol,
            map_name: info.map_name,
            game_session_manifest: info.game_session_manifest.map(|bytes| bytes.to_vec()),
        }
    }
}

#[cfg(test)]
#[test]
fn server_metadata_keeps_protocol_map_and_manifest_for_analysis_context() {
    let info: contract::ServerInfo = csgoproto::CsvcMsgServerInfo {
        tick_interval: Some(1. / 64.),
        protocol: Some(14000),
        map_name: Some("de_fixture".into()),
        game_session_manifest: Some(vec![0, 128, 255].into()),
        ..Default::default()
    }
    .into();
    assert_eq!(info.protocol, Some(14000));
    assert_eq!(info.map_name.as_deref(), Some("de_fixture"));
    assert_eq!(info.game_session_manifest, Some(vec![0, 128, 255]));
    assert_eq!(info.tick_interval, Some(1. / 64.));
}
