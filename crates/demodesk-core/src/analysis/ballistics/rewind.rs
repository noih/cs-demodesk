//! Exact recorded command association only; this does not reconstruct a lag-compensated pose.
use csgoproto::{CsgoInputHistoryEntryPb, CsgoUserCmdPb};
use prost::Message;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

/// Only replicated controls from a complete, build-qualified convar stream.
/// Server-only unlag, stuck correction and per-player overrides remain separate.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize)]
pub struct ReplicatedPolicy {
    pub max_unlag: f32,
    pub use_full_interp: bool,
    pub force_full_interp: bool,
    pub force_target_time: bool,
    pub filter_player_simulation_time: bool,
    pub command_queue_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TickPair {
    pub tick: i32,
    pub fraction: f32,
}
#[derive(Debug)]
pub struct Command {
    pub raw_index: usize,
    pub packet_tick: i32,
    pub net_tick: u32,
    pub ordinal: u32,
    pub player_slot: i32,
    pub command_number: i32,
    pub server_tick_executed: i32,
    pub client_tick: Option<i32>,
    pub protobuf: CsgoUserCmdPb,
}
#[derive(Debug)]
pub struct Selected<'a> {
    pub command: &'a Command,
    pub history: &'a CsgoInputHistoryEntryPb,
    pub history_index: usize,
    pub message_tick: i32,
    pub attack_time: TickPair,
    pub render_time: TickPair,
    pub player_time: TickPair,
}
/// Qualification supplied by the scene consumer, never inferred from command timestamps.
#[derive(Clone, Copy, Debug)]
pub enum ExecutionQualification {
    Unqualified,
    /// Ordinary controller simulation, nonzero duration, unpaused, complete tick interval;
    /// the native input validation (which also uses preceding movement state) succeeded.
    OrdinaryUnpausedFullTick {
        total_paused_ticks: i32,
        input_substeps_validated: bool,
    },
}

#[derive(Clone, Copy, Debug)]
pub enum Deadline {
    Missing,
    /// The caller proved this deadline cannot schedule a callback in this command.
    NotApplicable,
    /// Complete tick/fraction pair in the game's pause-adjusted clock domain.
    Recorded(TickPair),
}

/// All possible native weapon callback times, in raw execution-clock coordinates.
/// These are a conservative set of scheduled callbacks, not a chosen firing time.
/// Primary, secondary and postponed-fire deadlines must describe pre-fire weapon state.
pub fn execution_candidates(
    selected: &Selected<'_>,
    qualification: ExecutionQualification,
    deadlines: [Deadline; 3],
) -> Option<Vec<TickPair>> {
    let ExecutionQualification::OrdinaryUnpausedFullTick {
        total_paused_ticks,
        input_substeps_validated: true,
    } = qualification
    else {
        return None;
    };
    if total_paused_ticks < 0 {
        return None;
    }
    let raw_start = selected.command.server_tick_executed.checked_sub(1)?;
    let game_start = raw_start.checked_sub(total_paused_ticks)?;
    let game_end = game_start.checked_add(1)?;
    if game_start < 0 {
        return None;
    }
    let base = selected.command.protobuf.base.as_ref()?;
    if base.subtick_moves.len() > 32 {
        return None;
    }
    let mut fractions = Vec::with_capacity(base.subtick_moves.len() + 3);
    let mut previous = 0.0;
    for step in &base.subtick_moves {
        let when = step.when?;
        if !when.is_finite() || when < previous || !(0.0..=1.0).contains(&when) {
            return None;
        }
        previous = when;
        // Missing button means an analog-only entry in the native protobuf default.
        let button = step.button.unwrap_or(0);
        if button != 0 && (!button.is_power_of_two() || button & 0xe1f == 0) {
            return None;
        }
        if button & 0x801 != 0 {
            step.pressed?;
            // Keep the two native f32 operations; this is not arbitrary decimal rounding.
            let shifted = when + 131_072.0_f32;
            fractions.push(shifted - 131_072.0_f32);
        }
    }
    for deadline in deadlines {
        let deadline = match deadline {
            Deadline::Missing => return None,
            Deadline::NotApplicable => continue,
            Deadline::Recorded(value) => value,
        };
        let mut tick = deadline.tick;
        let mut fraction = deadline.fraction;
        if !fraction.is_finite() || !(0.0..=1.0).contains(&fraction) {
            return None;
        }
        if fraction == 1.0 {
            tick = tick.checked_add(1)?;
            fraction = 0.0;
        }
        // Native pair comparison is strictly after the start and includes the end.
        if (tick > game_start || (tick == game_start && fraction > 0.0))
            && (tick < game_end || (tick == game_end && fraction == 0.0))
        {
            // The native insertion receives the normalized deadline fraction alone.
            fractions.push(fraction);
        }
    }
    fractions.sort_by(f32::total_cmp);
    fractions.dedup_by(|a, b| *a == *b);
    let mut candidates = Vec::with_capacity(fractions.len());
    for fraction in fractions {
        let (tick, fraction) = if fraction == 1.0 {
            (raw_start.checked_add(1)?, 0.0)
        } else {
            (raw_start, fraction)
        };
        if tick.checked_sub(total_paused_ticks)? == selected.message_tick {
            candidates.push(TickPair { tick, fraction });
        }
    }
    (!candidates.is_empty()).then_some(candidates)
}

// Packet, full pawn handle, attack type, render tick, exact render fraction bits.
type Key = (i32, u32, u8, i32, u32);
#[derive(Default)]
pub struct Index {
    commands: Vec<Command>,
    matches: BTreeMap<Key, Option<(usize, usize)>>,
    invalid_packets: BTreeSet<i32>,
    invalid_slots: BTreeSet<(i32, i32)>,
    invalid_clock: bool,
}
fn integer(value: &Value) -> Option<i32> {
    i32::try_from(value.as_i64()?).ok()
}
fn unsigned(value: &Value) -> Option<u32> {
    u32::try_from(value.as_u64()?).ok()
}
fn pair(tick: Option<i32>, fraction: Option<f32>) -> Option<TickPair> {
    let (tick, fraction) = (tick?, fraction?);
    (tick >= 0 && fraction.is_finite() && (0.0..=1.0).contains(&fraction))
        .then_some(TickPair { tick, fraction })
}
fn event_pair(event: &Value, tick: &str, fraction: &str) -> Option<TickPair> {
    pair(
        integer(&event[tick]),
        event[fraction].as_f64().map(|v| v as f32),
    )
}
fn decode_hex(text: &str) -> Option<Vec<u8>> {
    let digit = |b| match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    };
    if text.len() % 2 != 0 || text.len() > 16 * 1024 * 1024 {
        return None;
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|b| Some(digit(b[0])? * 16 + digit(b[1])?))
        .collect()
}
impl Index {
    pub fn from_raw(events: &[Value]) -> Self {
        let mut index = Self::default();
        for (raw_index, event) in events.iter().enumerate() {
            if event["event_name"].as_str() != Some("analysis_user_cmd") {
                continue;
            }
            let Some(tick) = integer(&event["tick"]) else {
                index.invalid_clock = true;
                continue;
            };
            // Signon packets are outside the live event clock domain.
            if tick < 0 {
                continue;
            }
            let slot = integer(&event["player_slot"]).filter(|s| *s >= 0);
            let decoded = if event["invalid"].is_null() {
                Self::command(event, raw_index, tick)
            } else {
                None
            };
            let Some(command) = decoded else {
                if let Some(slot) = slot {
                    index.invalid_slots.insert((tick, slot));
                } else {
                    index.invalid_packets.insert(tick);
                }
                continue;
            };
            let pawn = command
                .protobuf
                .base
                .as_ref()
                .and_then(|b| b.pawn_entity_handle);
            let mut keys = Vec::new();
            let mut malformed = pawn.is_none() || pawn == Some(0x00ff_ffff);
            for (attack, selected) in [
                command.protobuf.attack1_start_history_index,
                command.protobuf.attack2_start_history_index,
            ]
            .into_iter()
            .enumerate()
            {
                let Some(selected) = selected.filter(|v| *v >= 0) else {
                    continue;
                };
                let history = command.protobuf.input_history.get(selected as usize);
                let render =
                    history.and_then(|h| pair(h.render_tick_count, h.render_tick_fraction));
                let player =
                    history.and_then(|h| pair(h.player_tick_count, h.player_tick_fraction));
                if let (Some(pawn), Some(render), Some(_)) = (pawn, render, player) {
                    keys.push((
                        (
                            tick,
                            pawn,
                            attack as u8,
                            render.tick,
                            render.fraction.to_bits(),
                        ),
                        selected as usize,
                    ));
                } else {
                    malformed = true;
                }
            }
            if malformed {
                index.invalid_slots.insert((tick, command.player_slot));
                continue;
            }
            let position = index.commands.len();
            index.commands.push(command);
            for (key, history) in keys {
                index
                    .matches
                    .entry(key)
                    .and_modify(|v| *v = None)
                    .or_insert(Some((position, history)));
            }
        }
        index
    }
    fn command(event: &Value, raw_index: usize, packet_tick: i32) -> Option<Command> {
        let bytes = decode_hex(event["protobuf_hex"].as_str()?)?;
        Some(Command {
            raw_index,
            packet_tick,
            net_tick: unsigned(&event["net_tick"])?,
            ordinal: unsigned(&event["ordinal"])?,
            player_slot: integer(&event["player_slot"]).filter(|s| *s >= 0)?,
            command_number: integer(&event["command_number"])?,
            server_tick_executed: integer(&event["server_tick_executed"])?,
            client_tick: if event["client_tick"].is_null() {
                None
            } else {
                Some(integer(&event["client_tick"])?)
            },
            protobuf: CsgoUserCmdPb::decode(bytes.as_slice()).ok()?,
        })
    }
    pub fn for_bullet(&self, event: &Value) -> Option<Selected<'_>> {
        if self.invalid_clock || event["event_name"].as_str() != Some("fire_bullets") {
            return None;
        }
        let tick = integer(&event["tick"])?;
        if self.invalid_packets.contains(&tick) {
            return None;
        }
        let pawn = unsigned(&event["player"])?;
        let attack = unsigned(&event["attack_type"])?;
        if attack > 1 {
            return None;
        }
        let render_time = event_pair(event, "render_tick_count", "render_tick_frac")?;
        let attack_time = event_pair(event, "attack_tick_count", "attack_tick_frac")?;
        let (command_index, history_index) = (*self.matches.get(&(
            tick,
            pawn,
            attack as u8,
            render_time.tick,
            render_time.fraction.to_bits(),
        ))?)?;
        let command = self.commands.get(command_index)?;
        if self.invalid_slots.contains(&(tick, command.player_slot)) {
            return None;
        }
        let history = command.protobuf.input_history.get(history_index)?;
        Some(Selected {
            command,
            history,
            history_index,
            message_tick: integer(&event["message_tick"])?,
            attack_time,
            render_time,
            player_time: pair(history.player_tick_count, history.player_tick_fraction)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use csgoproto::{CBaseUserCmdPb, CsgoInterpolationInfoPb, CsgoInterpolationInfoPbCl};
    use serde_json::json;

    fn command() -> CsgoUserCmdPb {
        let history = CsgoInputHistoryEntryPb {
            render_tick_count: Some(3821),
            render_tick_fraction: Some(f32::from_bits(0x3ebd5cf3)),
            player_tick_count: Some(3832),
            player_tick_fraction: Some(0.25),
            cl_interp: Some(CsgoInterpolationInfoPbCl { frac: Some(0.75) }),
            sv_interp0: Some(CsgoInterpolationInfoPb {
                src_tick: Some(3819),
                dst_tick: Some(3821),
                frac: Some(0.5),
            }),
            sv_interp1: Some(CsgoInterpolationInfoPb {
                src_tick: Some(3821),
                dst_tick: Some(3822),
                frac: Some(0.125),
            }),
            ..Default::default()
        };
        CsgoUserCmdPb {
            base: Some(CBaseUserCmdPb {
                pawn_entity_handle: Some(12894567),
                ..Default::default()
            }),
            input_history: vec![
                history,
                CsgoInputHistoryEntryPb {
                    render_tick_count: Some(3822),
                    ..history
                },
            ],
            attack1_start_history_index: Some(0),
            attack2_start_history_index: Some(1),
            ..Default::default()
        }
    }
    fn record(command: &CsgoUserCmdPb) -> Value {
        let hex: String = command
            .encode_to_vec()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        json!({"event_name":"analysis_user_cmd", "tick":2247, "net_tick":3832,
            "ordinal":2, "player_slot":0, "command_number":3357, "server_tick_executed":3832,
            "client_tick":null, "protobuf_hex":hex, "invalid":null})
    }
    fn bullet() -> Value {
        json!({"event_name":"fire_bullets", "tick":2247, "message_tick":3831,
            "player":12894567, "attack_type":0, "render_tick_count":3821,
            "render_tick_frac":f32::from_bits(0x3ebd5cf3), "attack_tick_count":3832,
            "attack_tick_frac":f32::from_bits(0x3ea840e0)})
    }
    #[test]
    fn exact_join_preserves_history_and_distinct_time_domains() {
        let command = command();
        let mut second = command.clone();
        second.input_history[0].render_tick_count = Some(3823);
        second.attack2_start_history_index = None;
        let mut record2 = record(&second);
        record2["ordinal"] = json!(3);
        record2["command_number"] = json!(3358);
        let events =
            crate::analysis::event_context::Events::from_raw(vec![record(&command), record2]);
        let first = bullet();
        let selected = events.rewind_for_bullet(&first).unwrap();
        assert_eq!(selected.command.raw_index, 0);
        assert_eq!(selected.command.server_tick_executed, 3832);
        assert_eq!(selected.message_tick, 3831);
        assert_eq!(selected.command.client_tick, None);
        assert_eq!(selected.attack_time.fraction.to_bits(), 0x3ea840e0);
        assert_eq!(selected.player_time.fraction, 0.25);
        assert_eq!(*selected.history, command.input_history[0]);
        assert!(std::ptr::eq(
            selected.history,
            &selected.command.protobuf.input_history[0]
        ));
        let mut another = first.clone();
        another["render_tick_count"] = json!(3823);
        assert_eq!(
            events
                .rewind_for_bullet(&another)
                .unwrap()
                .command
                .command_number,
            3358
        );
        another["attack_type"] = json!(1);
        another["render_tick_count"] = json!(3822);
        assert_eq!(events.rewind_for_bullet(&another).unwrap().history_index, 1);
        for (key, value) in [
            ("tick", json!(2248)),
            ("player", json!(12894567u32 + 32768)),
            ("attack_type", json!(2)),
            ("render_tick_frac", json!(0.5)),
            ("message_tick", Value::Null),
        ] {
            let mut wrong = first.clone();
            wrong[key] = value;
            assert!(events.rewind_for_bullet(&wrong).is_none(), "{key}");
        }
    }
    #[test]
    fn ambiguous_invalid_and_absent_inputs_are_not_guessed() {
        let command = command();
        let valid = record(&command);
        let fire = bullet();
        assert!(Index::from_raw(&[valid.clone(), valid.clone()])
            .for_bullet(&fire)
            .is_none());
        for (key, value) in [
            ("invalid", json!("invalid_baseline_or_command")),
            ("protobuf_hex", json!("ff")),
            ("protobuf_hex", json!("xy")),
            ("command_number", Value::Null),
            ("server_tick_executed", Value::Null),
        ] {
            let mut bad = valid.clone();
            bad[key] = value;
            assert!(
                Index::from_raw(&[valid.clone(), bad])
                    .for_bullet(&fire)
                    .is_none(),
                "{key}"
            );
        }
        let mut invalid = valid.clone();
        invalid["invalid"] = json!("invalid_baseline_or_command");
        invalid["player_slot"] = json!(1);
        assert!(Index::from_raw(&[invalid.clone(), valid.clone()])
            .for_bullet(&fire)
            .is_some());
        invalid["player_slot"] = Value::Null;
        assert!(Index::from_raw(&[invalid.clone(), valid.clone()])
            .for_bullet(&fire)
            .is_none());
        invalid["tick"] = json!(2246);
        assert!(Index::from_raw(&[invalid, valid])
            .for_bullet(&fire)
            .is_some());
        for choice in 0..4 {
            let mut missing = command.clone();
            match choice {
                0 => missing.attack1_start_history_index = None,
                1 => missing.input_history[0].render_tick_fraction = None,
                2 => missing.input_history[0].player_tick_count = None,
                _ => missing.attack1_start_history_index = Some(7),
            }
            assert!(Index::from_raw(&[record(&missing)])
                .for_bullet(&fire)
                .is_none());
        }
    }
    #[test]
    fn finite_clocks_require_complete_qualified_inputs_and_preserve_domains() {
        use csgoproto::CSubtickMoveStep;
        let mut command = command();
        let base = command.base.as_mut().unwrap();
        base.prediction_offset_ticks_x256 = Some(2806);
        base.subtick_moves = vec![
            CSubtickMoveStep {
                button: Some(1),
                pressed: Some(true),
                when: Some(0.42),
                ..Default::default()
            },
            CSubtickMoveStep {
                button: Some(0x800),
                pressed: Some(false),
                when: Some(1.0),
                ..Default::default()
            },
        ];
        let index = Index::from_raw(&[record(&command)]);
        let mut fire = bullet();
        fire["message_tick"] = json!(3781);
        let selected = index.for_bullet(&fire).unwrap();
        let qualification = ExecutionQualification::OrdinaryUnpausedFullTick {
            total_paused_ticks: 50,
            input_substeps_validated: true,
        };
        let deadlines = [
            Deadline::Recorded(TickPair {
                tick: 3781,
                fraction: 0.8,
            }),
            Deadline::Recorded(TickPair {
                tick: 3782,
                fraction: 0.0,
            }),
            Deadline::NotApplicable,
        ];
        assert_eq!(
            execution_candidates(&selected, qualification, deadlines).unwrap(),
            vec![
                TickPair {
                    tick: 3831,
                    fraction: 0.0
                },
                TickPair {
                    tick: 3831,
                    fraction: 0.421875
                },
                TickPair {
                    tick: 3831,
                    fraction: 0.8
                },
            ]
        );
        assert!(
            execution_candidates(&selected, ExecutionQualification::Unqualified, deadlines)
                .is_none()
        );
        assert!(execution_candidates(&selected, qualification, [Deadline::Missing; 3]).is_none());
        assert!(execution_candidates(
            &selected,
            ExecutionQualification::OrdinaryUnpausedFullTick {
                total_paused_ticks: 50,
                input_substeps_validated: false,
            },
            deadlines
        )
        .is_none());
        // A deadline exactly at the start is excluded; one after the end is excluded.
        let outside = [
            Deadline::Recorded(TickPair {
                tick: 3781,
                fraction: 0.0,
            }),
            Deadline::Recorded(TickPair {
                tick: 3782,
                fraction: 0.01,
            }),
            Deadline::NotApplicable,
        ];
        assert_eq!(
            execution_candidates(&selected, qualification, outside).unwrap(),
            vec![TickPair {
                tick: 3831,
                fraction: 0.421875
            }]
        );
        fire["message_tick"] = json!(3782);
        let selected = index.for_bullet(&fire).unwrap();
        assert_eq!(
            execution_candidates(&selected, qualification, deadlines).unwrap(),
            vec![TickPair {
                tick: 3832,
                fraction: 0.0
            }]
        );
        // Explicitly absent input fraction cannot become a zero-time callback.
        command.base.as_mut().unwrap().subtick_moves[0].when = None;
        let missing = Index::from_raw(&[record(&command)]);
        assert!(execution_candidates(
            &missing.for_bullet(&fire).unwrap(),
            qualification,
            deadlines
        )
        .is_none());
    }

    #[test]
    fn signon_tick_does_not_invalidate_live_commands() {
        let valid = record(&command());
        let mut signon = valid.clone();
        signon["tick"] = json!(-1);
        signon["invalid"] = json!("invalid_baseline_or_command");
        assert!(Index::from_raw(&[signon.clone(), valid.clone()])
            .for_bullet(&bullet())
            .is_some());
        signon["tick"] = Value::Null;
        assert!(Index::from_raw(&[signon, valid])
            .for_bullet(&bullet())
            .is_none());
    }

    #[test]
    fn render_fraction_bits_and_optional_interpolation_survive() {
        let mut command = command();
        command.input_history[0].render_tick_fraction = Some(-0.0);
        command.input_history[0].sv_interp1 = None;
        let index = Index::from_raw(&[record(&command)]);
        let mut fire = bullet();
        fire["render_tick_frac"] = json!(-0.0);
        let selected = index.for_bullet(&fire).unwrap();
        assert_eq!(selected.render_time.fraction.to_bits(), (-0.0f32).to_bits());
        assert!(selected.history.sv_interp1.is_none());
        fire["render_tick_frac"] = json!(0.0);
        assert!(index.for_bullet(&fire).is_none());
    }
}
