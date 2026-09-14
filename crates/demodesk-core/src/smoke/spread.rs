//! Deterministic pellet directions for the qualified native FMA spread policy.
//! Directions retain their original magnitude: ballistics multiply them by weapon range.

#[derive(Debug, Clone, Copy)]
pub struct Inputs {
    /// Original FireBullets seed; the native caller adds one.
    pub seed: u32,
    pub item_def_index: u32,
    pub mode: u32,
    pub inaccuracy: f32,
    pub spread: f32,
    pub recoil: f32,
    /// FireBullets angles already include aim punch.
    pub angles: [f32; 3],
    pub weapon: WeaponPattern,
    pub policy: Policy,
}

#[derive(Debug, Clone, Copy)]
pub struct WeaponPattern {
    /// Resource m_nNumBullets, not a count inferred from observed hits.
    pub pellets: usize,
    /// Resource m_nSpreadSeed, required for multi-pellet weapons.
    pub pattern_seed: Option<i32>,
}

/// Explicit recorded/native settings; missing settings must not be guessed by callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub struct Policy {
    pub patterns_enabled: bool,
    pub only_up: bool,
    pub maximum_inaccuracy: bool,
}

/// Policy captured immediately before this FireBullets message in wire order.
#[derive(Debug, serde::Serialize)]
pub struct FirePolicy {
    pub tick: i32,
    pub player: u32,
    pub seed: u32,
    pub item_def_index: u32,
    pub policy: Policy,
    pub rewind: Option<crate::analysis::ballistics::rewind::ReplicatedPolicy>,
}

/// Native signon sends only non-default replicated settings. Require a complete
/// source sequence and the qualified protocol/patch before applying its defaults.
pub fn policies_for_demo(bytes: &[u8]) -> anyhow::Result<Vec<FirePolicy>> {
    const NAMES: [&str; 9] = [
        "weapon_accuracy_shotgun_spread_patterns",
        "weapon_debug_inaccuracy_only_up",
        "weapon_debug_max_inaccuracy",
        "sv_maxunlag",
        "sv_csgo_shoot_use_full_interp",
        "sv_csgo_shoot_force_full_interp",
        "sv_csgo_shoot_force_use_target_time",
        "filter_player_simulation_time",
        "cq_enable",
    ];
    let source = parser::first_pass::convars::read(bytes, &NAMES).map_err(anyhow::Error::msg)?;
    let header = source.header();
    anyhow::ensure!(
        header.patch_version == Some(14181)
            && header.demo_version_name.as_deref() == Some("valve_demo_2"),
        "unsupported spread policy source build"
    );
    source
        .fires()
        .iter()
        .map(|fire| {
            let boolean = |name: &str, default: bool| -> anyhow::Result<bool> {
                Ok(match fire.values.get(name).map(String::as_str) {
                    None => default,
                    Some("true" | "1") => true,
                    Some("false" | "0") => false,
                    Some(_) => anyhow::bail!("invalid replicated spread policy value"),
                })
            };
            let rewind = (|| -> anyhow::Result<_> {
                let max_unlag = fire
                    .values
                    .get(NAMES[3])
                    .map(|v| v.parse::<f32>())
                    .transpose()?
                    .unwrap_or(1.);
                anyhow::ensure!(
                    max_unlag.is_finite() && (0. ..=1.).contains(&max_unlag),
                    "invalid replicated rewind limit"
                );
                Ok(crate::analysis::ballistics::rewind::ReplicatedPolicy {
                    max_unlag,
                    use_full_interp: boolean(NAMES[4], true)?,
                    force_full_interp: boolean(NAMES[5], false)?,
                    force_target_time: boolean(NAMES[6], false)?,
                    filter_player_simulation_time: boolean(NAMES[7], true)?,
                    command_queue_enabled: boolean(NAMES[8], true)?,
                })
            })()
            .ok();
            Ok(FirePolicy {
                rewind,
                tick: fire.tick,
                player: fire.player,
                seed: fire.seed,
                item_def_index: fire.item_def_index,
                policy: Policy {
                    patterns_enabled: boolean(NAMES[0], true)?,
                    only_up: boolean(NAMES[1], false)?,
                    maximum_inaccuracy: boolean(NAMES[2], false)?,
                },
            })
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    InvalidInputs,
    MissingPatternSeed,
    UnsupportedAngle,
    UnsupportedPolicy,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidInputs => "invalid spread inputs",
            Self::MissingPatternSeed => "missing weapon spread pattern seed",
            Self::UnsupportedAngle => "spread angle exceeds the qualified domain",
            Self::UnsupportedPolicy => "spread policy is not supported",
        })
    }
}
impl std::error::Error for Error {}

struct Random {
    state: i32,
    previous: i32,
    table: [i32; 32],
}
impl Random {
    fn new(seed: u32) -> Self {
        let signed = seed as i32;
        Self {
            state: signed.wrapping_abs().wrapping_neg(),
            previous: 0,
            table: [0; 32],
        }
    }
    fn advance(&mut self) {
        let k = self.state / 127773;
        self.state = self
            .state
            .wrapping_mul(16807)
            .wrapping_sub(k.wrapping_mul(2147483647));
        if self.state < 0 {
            self.state = self.state.wrapping_add(2147483647);
        }
    }
    fn integer(&mut self) -> i32 {
        if self.state <= 0 || self.previous == 0 {
            self.state = self.state.wrapping_neg().max(1);
            for j in (0..40).rev() {
                self.advance();
                if j < 32 {
                    self.table[j] = self.state;
                }
            }
            self.previous = self.table[0];
        }
        self.advance();
        let i = (self.previous >> 26) as usize;
        let result = self.table[i];
        self.previous = result;
        self.table[i] = self.state;
        result
    }
    fn float(&mut self, lo: f32, hi: f32) -> f32 {
        let fraction = ((self.integer() as f32) * (1.0 / 2147483647.0_f32)).min(0.9999999);
        fraction * (hi - lo) + lo
    }
}
#[path = "spread_trig.rs"]
mod native_trig;
use native_trig::{cos, sin};
// Native radial overrides: revolver secondary and the first Negev recoil steps.
fn radial(mut x: f32, p: &Inputs) -> f32 {
    if p.item_def_index == 64 && p.mode == 1 {
        x = 1.0 - x * x;
    }
    if p.item_def_index == 28 && p.recoil < 3.0 {
        let mut j = 3;
        loop {
            j -= 1;
            x *= x;
            if j as f32 <= p.recoil {
                break;
            }
        }
        x = 1.0 - x;
    }
    x
}
/// All candidate pellet directions. Identity is its fixed array index, never nearest-target selection.
/// Pinned native FMA trig validated bitwise on all captured inputs; recorded damage direction may normalize a different clipped vector.
pub fn raw_directions(p: Inputs) -> Result<Vec<[f32; 3]>, Error> {
    if p.weapon.pellets == 0
        || p.weapon.pellets > 64
        || !p
            .angles
            .into_iter()
            .chain([p.inaccuracy, p.spread, p.recoil])
            .all(f32::is_finite)
        || p.inaccuracy < 0.0
        || p.spread < 0.0
        || p.recoil < 0.0
    {
        return Err(Error::InvalidInputs);
    }
    if p.angles
        .iter()
        .any(|a| (a * (std::f32::consts::PI / 180.0)).abs() > 7.0)
    {
        return Err(Error::UnsupportedAngle);
    }
    if !p.policy.patterns_enabled || p.policy.only_up || p.policy.maximum_inaccuracy {
        return Err(Error::UnsupportedPolicy);
    }
    let tau = std::f32::consts::TAU;
    let table = if p.weapon.pellets > 1 {
        let seed = p.weapon.pattern_seed.ok_or(Error::MissingPatternSeed)?;
        let mut r = Random::new(seed as u32);
        let step = 1.0 / p.weapon.pellets as f32;
        let mut table = Vec::with_capacity(64);
        for i in 0..64 {
            let j = i % p.weapon.pellets;
            table.push((
                r.float(0.0, tau),
                r.float(j as f32 * step, (j + 1) as f32 * step)
                    .clamp(0.0, 1.0),
            ));
        }
        Some(table)
    } else {
        None
    };
    let [pitch, yaw, roll] = p.angles.map(|x| x * (std::f32::consts::PI / 180.0));
    let (sp, cp, sy, cy, sr, cr) = (
        sin(pitch),
        cos(pitch),
        sin(yaw),
        cos(yaw),
        sin(roll),
        cos(roll),
    );
    let forward = [cp * cy, cp * sy, -sp];
    // 1334A70 returns LEFT in its third argument (R8), not classic AngleVectors' right.
    let left = [(sr * sp) * cy - cr * sy, (sr * sp) * sy + cr * cy, sr * cp];
    let up = [(cr * sp) * cy + sr * sy, (cr * sp) * sy - sr * cy, cr * cp];
    let mut r = Random::new(p.seed.wrapping_add(1));
    let mut out = Vec::with_capacity(p.weapon.pellets);
    for i in 0..p.weapon.pellets {
        let u = radial(r.float(0.0, 1.0), &p) * p.inaccuracy;
        let t1 = r.float(0.0, tau);
        let index = (p.recoil.trunc() as usize)
            .saturating_mul(p.weapon.pellets)
            .saturating_add(i);
        let (t2, radius) = match table.as_ref().and_then(|t| t.get(index)) {
            Some(v) => *v,
            None => (r.float(0.0, tau), r.float(0.0, 1.0)),
        };
        let v = radial(radius, &p) * p.spread;
        let a = cos(t1) * u + cos(t2) * v;
        let b = sin(t1) * u + sin(t2) * v;
        let raw = std::array::from_fn::<_, 3, _>(|j| (forward[j] - left[j] * a) + up[j] * b);
        if !raw.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidInputs);
        }
        out.push(raw);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn input() -> Inputs {
        Inputs {
            seed: 0,
            item_def_index: 7,
            mode: 0,
            inaccuracy: 0.,
            spread: 0.,
            recoil: 0.,
            angles: [0.; 3],
            weapon: WeaponPattern {
                pellets: 1,
                pattern_seed: None,
            },
            policy: Policy {
                patterns_enabled: true,
                only_up: false,
                maximum_inaccuracy: false,
            },
        }
    }

    fn policy_demo(
        reordered: bool,
        initial: bool,
        reset: bool,
        value: &str,
        rewind: &[(&str, &str)],
    ) -> Vec<u8> {
        use prost::Message;
        fn vi(out: &mut Vec<u8>, mut n: u32) {
            while n >= 128 {
                out.push((n as u8) | 128);
                n >>= 7;
            }
            out.push(n as u8);
        }
        fn frame(out: &mut Vec<u8>, cmd: u32, tick: u32, data: &[u8]) {
            vi(out, cmd);
            vi(out, tick);
            vi(out, data.len() as u32);
            out.extend(data);
        }
        fn packet(messages: Vec<(u32, Vec<u8>)>) -> Vec<u8> {
            let mut bits = Vec::new();
            let mut put = |n: u32, width: usize| {
                for i in 0..width {
                    bits.push(((n >> i) & 1) as u8);
                }
            };
            for (kind, data) in messages {
                if kind < 16 {
                    put(kind, 6);
                } else {
                    put((kind & 15) | 32, 6);
                    put(kind >> 4, 8);
                }
                let mut size = Vec::new();
                vi(&mut size, data.len() as u32);
                for byte in size.into_iter().chain(data) {
                    put(u32::from(byte), 8);
                }
            }
            let mut bytes = vec![0; (bits.len() + 7) / 8];
            for (i, b) in bits.into_iter().enumerate() {
                bytes[i / 8] |= b << (i % 8);
            }
            csgoproto::CDemoPacket {
                data: Some(bytes.into()),
                ..Default::default()
            }
            .encode_to_vec()
        }
        let cvar = |name: &str, v: &str| {
            (
                6,
                csgoproto::CnetMsgSetConVar {
                    convars: Some(csgoproto::CMsgCVars {
                        cvars: vec![csgoproto::c_msg_c_vars::CVar {
                            name: Some(name.into()),
                            value: Some(v.into()),
                        }],
                    }),
                }
                .encode_to_vec(),
            )
        };
        let state = |n| {
            (
                7,
                csgoproto::CnetMsgSignonState {
                    signon_state: Some(n),
                    ..Default::default()
                }
                .encode_to_vec(),
            )
        };
        let fire = |seed| {
            (
                452,
                csgoproto::CMsgTeFireBullets {
                    player: Some(123),
                    seed: Some(seed),
                    item_def_index: Some(7),
                    ..Default::default()
                }
                .encode_to_vec(),
            )
        };
        let mut out = b"PBDEMS2\0".to_vec();
        out.resize(16, 0);
        frame(
            &mut out,
            1,
            u32::MAX,
            &csgoproto::CDemoFileHeader {
                patch_version: Some(14181),
                demo_version_name: Some("valve_demo_2".into()),
                ..Default::default()
            }
            .encode_to_vec(),
        );
        let empty = (
            6,
            csgoproto::CnetMsgSetConVar {
                convars: Some(csgoproto::CMsgCVars { cvars: vec![] }),
            }
            .encode_to_vec(),
        );
        let signon = if !initial {
            vec![state(3), state(4), state(5)]
        } else if reordered {
            vec![state(3), empty, state(4), state(5)]
        } else {
            vec![empty, state(3), state(4), state(5)]
        };
        frame(&mut out, 8, u32::MAX, &packet(signon));
        let mut messages = vec![fire(1), cvar("weapon_debug_inaccuracy_only_up", value)];
        messages.extend(rewind.iter().map(|(name, value)| cvar(name, value)));
        messages.extend([
            fire(2),
            cvar("weapon_debug_inaccuracy_only_up", "0"),
            fire(3),
            state(if reset { 3 } else { 6 }),
        ]);
        frame(&mut out, 7, 1, &packet(messages));
        frame(&mut out, 0, 2, &[]);
        out
    }
    #[test]
    fn policy_provenance_preserves_wire_order_and_return_to_default() {
        let result = policies_for_demo(&policy_demo(false, true, false, "1", &[])).unwrap();
        assert_eq!(
            result.iter().map(|f| f.policy.only_up).collect::<Vec<_>>(),
            [false, true, false]
        );
        assert!(result
            .iter()
            .all(|f| f.policy.patterns_enabled && !f.policy.maximum_inaccuracy));
        let default = result[0].rewind.unwrap();
        assert_eq!(default.max_unlag, 1.);
        assert!(default.filter_player_simulation_time && default.command_queue_enabled);
        assert!(
            default.use_full_interp && !default.force_full_interp && !default.force_target_time
        );
        let changed = policies_for_demo(&policy_demo(
            false,
            true,
            false,
            "1",
            &[
                ("sv_maxunlag", "0.5"),
                ("sv_csgo_shoot_use_full_interp", "0"),
                ("sv_csgo_shoot_force_full_interp", "1"),
                ("sv_csgo_shoot_force_use_target_time", "1"),
                ("filter_player_simulation_time", "0"),
                ("cq_enable", "0"),
            ],
        ))
        .unwrap();
        assert_eq!(changed[0].rewind, Some(default));
        let policy = changed[1].rewind.unwrap();
        assert_eq!(policy.max_unlag, 0.5);
        assert!(!policy.filter_player_simulation_time && !policy.command_queue_enabled);
        assert!(!policy.use_full_interp && policy.force_full_interp && policy.force_target_time);
        assert_eq!(changed[2].rewind, Some(policy));
        for invalid in ["NaN", "-1", "2"] {
            let bad = policies_for_demo(&policy_demo(
                false,
                true,
                false,
                "1",
                &[("sv_maxunlag", invalid)],
            ))
            .unwrap();
            assert!(bad[0].rewind.is_some());
            assert!(bad[1].rewind.is_none() && bad[1].policy.only_up);
        }
        assert!(policies_for_demo(&policy_demo(true, true, false, "1", &[])).is_err());
        assert!(policies_for_demo(&policy_demo(false, false, false, "1", &[])).is_err());
        assert!(policies_for_demo(&policy_demo(false, true, false, "invalid", &[])).is_err());
        assert!(policies_for_demo(&policy_demo(false, true, true, "1", &[])).is_err());
        let mut unknown = policy_demo(false, true, false, "1", &[]);
        let end = unknown.len() - 3;
        unknown[end] = 19;
        assert!(policies_for_demo(&unknown).is_err());
        let mut truncated = policy_demo(false, true, false, "1", &[]);
        truncated.pop();
        assert!(policies_for_demo(&truncated).is_err());
        assert!(policies_for_demo(b"PBDEMS2\0").is_err());
    }
    #[test]
    fn native_trig_samples_match_bits() {
        // Independent pinned tier0 SinCos capture, covering positive/negative quadrants.
        for (x, s, c) in [
            (3188288330, 3188261323, 1065202371),
            (1062046353, 1060645034, 1060229901),
            (3223464545, 3205639887, 3209832592),
            (1073113152, 1064311408, 3199310956),
            (1084037149, 3212513004, 1044921020),
        ] {
            assert_eq!(sin(f32::from_bits(x)).to_bits(), s);
            assert_eq!(cos(f32::from_bits(x)).to_bits(), c);
        }
    }
    #[test]
    fn rejects_unqualified_context_without_panicking() {
        let mut p = input();
        p.angles[0] = 1000.;
        assert_eq!(raw_directions(p), Err(Error::UnsupportedAngle));
        p = input();
        p.weapon.pellets = 6;
        assert_eq!(raw_directions(p), Err(Error::MissingPatternSeed));
        p = input();
        p.policy.patterns_enabled = false;
        assert_eq!(raw_directions(p), Err(Error::UnsupportedPolicy));
        p = input();
        p.inaccuracy = f32::NAN;
        assert_eq!(raw_directions(p), Err(Error::InvalidInputs));
    }
    #[test]
    fn native_server_spread_sample_matches_bits() {
        // Captured server AK47 generator output; its input seed is FireBullets seed + 1.
        let mut p = input();
        p.seed = 1911774012;
        p.inaccuracy = f32::from_bits(1003621114);
        p.spread = f32::from_bits(974997842);
        let actual = raw_directions(p).unwrap()[0];
        assert_eq!(actual.map(f32::to_bits), [1065353216, 981415845, 999798592]);
    }
    #[test]
    fn raw_direction_is_not_normalized() {
        let mut p = input();
        p.spread = 0.2;
        p.inaccuracy = 0.1;
        let d = raw_directions(p).unwrap()[0];
        assert_eq!(d[0], 1.);
        assert!(d.iter().map(|v| v * v).sum::<f32>() > 1.);
    }
}
