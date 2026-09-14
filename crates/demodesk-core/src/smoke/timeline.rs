//! Incremental smoke journal state shared by analysis and replay projection.
//! A missing byte never becomes zero, and an invalid lifetime never reuses stale density.
use super::{
    atlas::{Atlas, Parameters},
    density::Density,
    effects::Effects,
    sampling::Cloud,
};
use crate::analysis::compact;
use anyhow::{ensure, Context, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Default)]
pub struct Volume {
    pub origin: Option<[f32; 3]>,
    pub effect_tick: Option<i32>,
    pub did_effect: Option<bool>,
    present: bool,
    slots: Vec<Option<u8>>,
    size: Option<usize>,
    consumed: usize,
    density: Option<Density>,
    error: Option<String>,
    render: Option<RenderVolume>,
}
/// Optional canonical projection; no GPU upload clock or slot is inferred from the demo.
struct RenderVolume {
    atlas: Atlas,
    slot: u8,
    started: f32,
    updated: f32,
    seed_centres: Vec<[f32; 3]>,
}
struct RenderState {
    free_slots: Vec<u8>,
    effects: Effects,
}
impl Default for RenderState {
    fn default() -> Self {
        Self {
            free_slots: (0..16).rev().collect(),
            effects: Effects::default(),
        }
    }
}
impl Volume {
    /// Only a fully received current journal may be queried.
    pub fn density(&self) -> Option<&Density> {
        (self.present && self.error.is_none() && self.size == Some(self.consumed))
            .then_some(self.density.as_ref())
            .flatten()
    }
    fn empty_initial(&self) -> bool {
        // Native activation clears all cells before producing journal sequence zero.
        self.present
            && self.error.is_none()
            && self.size == Some(0)
            && self.consumed == 0
            && self.origin.is_some()
            && self.did_effect == Some(true)
            && self.effect_tick.is_some_and(|t| t > 0)
    }
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
    fn field(&mut self, name: &str, value: Option<&[u8]>) -> Result<()> {
        fn integer(v: &[u8]) -> Result<u32> {
            ensure!(
                v.len() == 5 && matches!(v[0], 1 | 2),
                "invalid smoke integer"
            );
            Ok(u32::from_le_bytes(v[1..].try_into()?))
        }
        fn boolean(v: &[u8]) -> Result<bool> {
            ensure!(
                v.len() == 2 && v[0] == 0 && v[1] <= 1,
                "invalid smoke boolean"
            );
            Ok(v[1] != 0)
        }
        match name {
            "$present" => self.present = value.map(boolean).transpose()?.unwrap_or(false),
            "m_bDidSmokeEffect" => self.did_effect = value.map(boolean).transpose()?,
            "m_nSmokeEffectTickBegin" => {
                self.effect_tick = value
                    .map(|v| -> Result<i32> {
                        let bits = integer(v)?;
                        Ok(if v[0] == 2 {
                            bits as i32
                        } else {
                            i32::try_from(bits)?
                        })
                    })
                    .transpose()?
            }
            "m_nVoxelFrameDataSize" => {
                let size = value.map(integer).transpose()?.map(|n| n as usize);
                ensure!(size.is_none_or(|n| n <= 65536), "oversized smoke journal");
                ensure!(
                    size.is_some_and(|n| n >= self.consumed) || self.consumed == 0,
                    "smoke journal shrank"
                );
                self.size = size;
            }
            "m_vSmokeDetonationPos" => {
                let origin = value
                    .map(|v| -> Result<[f32; 3]> {
                        ensure!(v.len() == 13 && v[0] == 7, "invalid smoke origin");
                        let origin = std::array::from_fn(|i| {
                            f32::from_le_bytes(
                                v[1 + i * 4..5 + i * 4]
                                    .try_into()
                                    .expect("validated vector"),
                            )
                        });
                        ensure!(
                            origin.iter().all(|n| n.is_finite()),
                            "non-finite smoke origin"
                        );
                        Ok(origin)
                    })
                    .transpose()?;
                ensure!(
                    self.density.is_none() || self.origin == origin,
                    "smoke origin changed after reconstruction"
                );
                self.origin = origin;
            }
            _ => {
                if let Some(slot) = name.strip_prefix("smokeVoxel/") {
                    let slot: usize = slot.parse().context("invalid smoke slot")?;
                    ensure!(slot < 65536, "oversized smoke slot");
                    let byte = value
                        .map(|v| -> Result<u8> { Ok(u8::try_from(integer(v)?)?) })
                        .transpose()?;
                    if self.slots.len() <= slot {
                        self.slots.resize(slot + 1, None);
                    }
                    ensure!(
                        slot >= self.consumed || self.slots[slot] == byte,
                        "consumed smoke byte changed"
                    );
                    self.slots[slot] = byte;
                }
            }
        }
        Ok(())
    }
    fn advance(&mut self, mut changed: impl FnMut(&Density) -> Result<()>) -> Result<()> {
        let (Some(origin), Some(size)) = (self.origin, self.size) else {
            return Ok(());
        };
        if !self.present || self.error.is_some() {
            return Ok(());
        }
        if size == 0 {
            return Ok(());
        }
        while self.consumed < size {
            let Some(header) = self.slots.get(self.consumed..self.consumed + 4) else {
                break;
            };
            let Some(header) = header.iter().copied().collect::<Option<Vec<_>>>() else {
                break;
            };
            let seq = u16::from_le_bytes(header[..2].try_into()?);
            let length = u16::from_le_bytes(header[2..].try_into()?) as usize;
            if length < 2 {
                self.error = Some("invalid smoke payload length".into());
                break;
            }
            let end = self.consumed + 4 + length;
            if end > size {
                break;
            }
            let Some(payload) = self.slots.get(self.consumed + 4..end) else {
                break;
            };
            let Some(payload) = payload.iter().copied().collect::<Option<Vec<_>>>() else {
                break;
            };
            if self.density.is_none() {
                self.density = Some(Density::new(origin)?);
            }
            let density = self.density.as_mut().expect("initialized density");
            if let Err(error) = density.step(seq, &payload) {
                self.error = Some(error.to_string());
                break;
            }
            self.consumed = end;
            if let Some(render) = &mut self.render {
                render.atlas.update(density)?;
                if render.seed_centres.is_empty() {
                    render.seed_centres.extend(density.seed_centres());
                }
            }
            changed(density)?;
        }
        Ok(())
    }
}
#[derive(Default)]
pub struct Timeline {
    volumes: BTreeMap<(i32, u32), Volume>,
    clock: Clock,
    last_net_tick: Option<u32>,
    before_update: bool,
    packet_net_tick: Option<u32>,
    render: Option<RenderState>,
}
#[derive(Default)]
struct Clock {
    paused_ticks: Option<i32>,
    pause_start: Option<i32>,
    paused: Option<bool>,
}
impl Clock {
    fn interval(&self, tick: i32) -> Option<[f32; 2]> {
        let paused_ticks = self.paused_ticks.filter(|t| *t >= 0)?;
        // The message tick is already in game time. Preserve native f32 operation order.
        let raw_tick = tick.checked_add(paused_ticks)?;
        let base = raw_tick as f32 * (1. / 64.);
        let pause = paused_ticks as f32 * (1. / 64.);
        let raw = [base, base + f32::from_bits(0x3f7f_ffff) * (1. / 64.)];
        let mut result = raw.map(|t| t - pause);
        if self.paused? {
            let start = self.pause_start?;
            if start > 0 {
                let boundary = start as f32 * (1. / 64.);
                let frozen = start.checked_sub(paused_ticks)? as f32 * (1. / 64.);
                if raw[0] >= boundary {
                    result = [frozen; 2];
                } else if raw[1] >= boundary {
                    result = [result[0].min(frozen), result[1].max(frozen)];
                }
            }
        }
        Some(result)
    }
}
impl Timeline {
    /// Opt in before the first packet. CPU-only analysis avoids atlas allocations.
    /// New scenes use first occurrence in compact `changed` order and a fixed
    /// free-list slot until removal. This is a canonical policy, not client order.
    pub fn with_render_state() -> Self {
        Self {
            render: Some(RenderState::default()),
            ..Self::default()
        }
    }
    /// Seeking requires replay from the beginning after resetting both projections
    /// and disturbances; old masks must not survive reassignment of scene slots.
    pub fn reset(&mut self) {
        *self = if self.render.is_some() {
            Self::with_render_state()
        } else {
            Self::default()
        };
    }
    /// Only qualified registration producers may populate these shared effects.
    /// Effect timestamps use network seconds, matching the canonical render clock.
    pub fn render_effects_mut(&mut self) -> Option<&mut Effects> {
        self.render.as_mut().map(|r| &mut r.effects)
    }
    pub fn render_effects(&self) -> Option<&Effects> {
        self.render.as_ref().map(|r| &r.effects)
    }
    fn render_ready(&self) -> Result<()> {
        ensure!(
            self.render.is_some(),
            "canonical smoke projection is disabled"
        );
        ensure!(self.last_net_tick.is_some(), "no canonical smoke packet");
        for v in self.volumes.values().filter(|v| v.present) {
            ensure!(v.error.is_none(), "invalid smoke projection: {:?}", v.error);
            if v.did_effect == Some(false) {
                continue;
            }
            ensure!(
                v.did_effect == Some(true)
                    && v.render.is_some()
                    && (v.density().is_some() || v.empty_initial()),
                "incomplete smoke projection"
            );
        }
        Ok(())
    }
    /// Borrow current canonical clouds in slot order for shared point/ray sampling.
    /// `now` is network seconds; missing journal data is an error, never transparency.
    pub fn render_clouds(&self, now: f32) -> Result<Vec<Cloud<'_>>> {
        self.render_ready()?;
        let last = self.last_net_tick.context("no canonical smoke packet")? as f32 / 64.;
        ensure!(
            now.is_finite() && now >= last,
            "canonical clock precedes current packet"
        );
        let mut clouds = Vec::new();
        for v in self
            .volumes
            .values()
            .filter(|v| v.present && v.did_effect == Some(true))
        {
            let r = v.render.as_ref().expect("validated projection");
            clouds.push(Cloud {
                atlas: &r.atlas,
                origin: v.origin.context("missing smoke origin")?,
                slot: r.slot,
                parameters: Parameters::new(now, r.started, r.updated)?,
            });
        }
        clouds.sort_by_key(|c| c.slot);
        Ok(clouds)
    }
    /// Same slots and fixed seed lists used by the HE registration producer.
    pub fn registered_smokes(&self) -> Result<Vec<super::he::RegisteredSmoke<'_>>> {
        self.render_ready()?;
        Self::scenes(&self.volumes)
    }
    fn scenes(
        volumes: &BTreeMap<(i32, u32), Volume>,
    ) -> Result<Vec<super::he::RegisteredSmoke<'_>>> {
        let mut scenes = Vec::new();
        for v in volumes
            .values()
            .filter(|v| v.present && v.did_effect == Some(true))
        {
            let r = v.render.as_ref().expect("validated projection");
            let origin = v.origin.context("missing smoke origin")?;
            scenes.push(super::he::RegisteredSmoke {
                slot: r.slot,
                bounds: [origin.map(|x| x - 320.), origin.map(|x| x + 320.)],
                seed_centres: &r.seed_centres,
            });
        }
        scenes.sort_by_key(|s| s.slot);
        Ok(scenes)
    }

    /// Register against this projection's slots without copying seed geometry.
    pub fn register_he(
        &mut self,
        explosion: super::he::Explosion,
        nearest: impl FnMut(
            [f32; 3],
            [f32; 3],
        ) -> Result<Option<crate::analysis::collision::asset::NearestHit>>,
    ) -> Result<bool> {
        self.render_ready()?;
        let scenes = Self::scenes(&self.volumes)?;
        super::he::register(
            &mut self.render.as_mut().expect("validated projection").effects,
            explosion,
            &scenes,
            nearest,
        )
    }
    pub fn volumes(&self) -> impl Iterator<Item = (&(i32, u32), &Volume)> {
        self.volumes.iter()
    }
    /// Enter weapon simulation before this packet's shared smoke-system update.
    /// A network gap cannot authorize reusing an older density snapshot.
    pub fn begin_packet(&mut self, frame: &compact::Frame<'_>) {
        self.packet_net_tick = Some(frame.net_tick);
        self.before_update =
            self.last_net_tick.and_then(|t| t.checked_add(1)) == Some(frame.net_tick);
        for id in frame.changed {
            let field = &frame.fields[*id as usize];
            if field.class != "CCSGameRulesProxy" {
                continue;
            }
            let value = frame.values.get(id);
            let integer = || {
                let v = value?;
                if v.len() != 5 || !matches!(v[0], 1 | 2) {
                    return None;
                }
                let bits = u32::from_le_bytes(v[1..].try_into().ok()?);
                if v[0] == 2 {
                    Some(bits as i32)
                } else {
                    i32::try_from(bits).ok()
                }
            };
            match field.name.rsplit('.').next() {
                Some("m_nTotalPausedTicks") => self.clock.paused_ticks = integer(),
                Some("m_nPauseStartTick") => self.clock.pause_start = integer(),
                Some("m_bGamePaused") => {
                    self.clock.paused = value
                        .and_then(|v| (v.len() == 2 && v[0] == 0 && v[1] <= 1).then(|| v[1] != 0))
                }
                _ => {}
            }
        }
    }
    /// No trajectory can cross smoke when every present volume is inactive or
    /// still has its certified empty activation state. Gaps remain unknown.
    pub fn has_no_density_at_fire(&self) -> bool {
        self.before_update
            && self.volumes.values().all(|v| {
                !v.present
                    || (v.error.is_none() && (v.did_effect == Some(false) || v.empty_initial()))
            })
    }
    /// Eligible samples for the directional estimate; growth/fade and packet gaps are excluded.
    pub fn stable_volumes_at_fire(&self, message_tick: i32) -> Option<Vec<(&Density, [f32; 3])>> {
        if !self.before_update || self.clock.paused != Some(false) {
            return None;
        }
        let raw = message_tick.checked_add(self.clock.paused_ticks?)?;
        let net = i32::try_from(self.packet_net_tick?).ok()?;
        if raw != net && raw.checked_add(1) != Some(net) {
            return None;
        }
        let now = self.clock.interval(message_tick)?;
        let mut volumes = Vec::new();
        for v in self.volumes.values().filter(|v| v.present) {
            if v.did_effect == Some(false) {
                continue;
            }
            if v.did_effect != Some(true) || v.error.is_some() {
                return None;
            }
            let start = v.effect_tick? as f32 / 64.;
            if now[0] - start < 1.5 || now[1] - start > 17. {
                return None;
            }
            volumes.push((v.density()?, v.origin?));
        }
        Some(volumes)
    }
    /// Certify the server CPU threshold for a normal recorded weapon shot.
    /// This is not rendered opacity or a ballistic stop/penetration query.
    pub fn crosses_at_fire(
        &self,
        message_tick: i32,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<Option<bool>> {
        let packet_matches = self.clock.paused == Some(false)
            && self
                .clock
                .paused_ticks
                .and_then(|p| message_tick.checked_add(p))
                .and_then(|t| u32::try_from(t).ok())
                .zip(self.packet_net_tick)
                .is_some_and(|(raw, net)| raw == net || raw.checked_add(1) == Some(net));
        if !packet_matches {
            return Ok(None);
        }
        let Some(now) = self
            .clock
            .interval(message_tick)
            .filter(|_| self.before_update)
        else {
            return Ok(None);
        };
        for volume in self.volumes.values().filter(|v| v.present) {
            if volume.did_effect == Some(false) {
                continue;
            }
            if volume.did_effect != Some(true)
                || volume.effect_tick.is_none_or(|t| t <= 0)
                || (volume.density().is_none() && !volume.empty_initial())
            {
                return Ok(None);
            }
        }
        super::crosses_during(
            start,
            end,
            now,
            self.volumes.values().filter_map(|v| {
                if !v.present || v.did_effect != Some(true) || v.empty_initial() {
                    return None;
                }
                Some((v.density()?, v.effect_tick? as f32 * (1. / 64.)))
            }),
        )
    }
    /// Advance every packet, including packets excluded from rule measurement.
    /// Callback receives each newly reconstructed sequence once, for incremental projections.
    pub fn update(
        &mut self,
        frame: &compact::Frame<'_>,
        mut changed: impl FnMut((i32, u32), &Density) -> Result<()>,
    ) -> Result<()> {
        if self.render.is_some() {
            ensure!(
                self.last_net_tick.is_none_or(|t| frame.net_tick >= t),
                "canonical replay moved backwards without reset"
            );
        }
        self.before_update = false;
        self.last_net_tick = Some(frame.net_tick);
        let mut arrival = Vec::new();
        let mut seen = BTreeSet::new();
        let mut dirty = BTreeSet::new();
        for id in frame.changed {
            let field = &frame.fields[*id as usize];
            if field.class != "CSmokeGrenadeProjectile" {
                continue;
            }
            let key = (field.entity, field.serial);
            if self.render.is_some() && seen.insert(key) {
                arrival.push(key);
            }
            let value = frame.values.get(id).map(Vec::as_slice);
            if field.name == "$present" && value.is_none() {
                if let Some(v) = self.volumes.remove(&key) {
                    if let (Some(state), Some(render)) = (&mut self.render, v.render) {
                        state.free_slots.push(render.slot);
                    }
                }
                dirty.remove(&key);
                continue;
            }
            if !(field.name.starts_with("smokeVoxel/")
                || matches!(
                    field.name.as_str(),
                    "$present"
                        | "m_bDidSmokeEffect"
                        | "m_nSmokeEffectTickBegin"
                        | "m_nVoxelFrameDataSize"
                        | "m_vSmokeDetonationPos"
                ))
            {
                continue;
            }
            if value.is_none() && !self.volumes.contains_key(&key) {
                continue;
            }
            let volume = self.volumes.entry(key).or_default();
            if let Err(error) = volume.field(&field.name, value) {
                volume.error = Some(error.to_string());
            }
            dirty.insert(key);
        }
        if let Some(state) = &mut self.render {
            for key in arrival {
                let Some(v) = self.volumes.get_mut(&key) else {
                    continue;
                };
                if !v.present || v.did_effect != Some(true) {
                    continue;
                }
                let (Some(origin), Some(tick)) = (v.origin, v.effect_tick) else {
                    continue;
                };
                if tick <= 0 {
                    continue;
                }
                let now = frame.net_tick as f32 / 64.;
                if v.render.is_none() {
                    ensure!(
                        v.consumed == 0,
                        "canonical projection enabled after journal consumption"
                    );
                    let slot = state
                        .free_slots
                        .pop()
                        .context("canonical smoke atlas capacity exceeded")?;
                    v.render = Some(RenderVolume {
                        atlas: Atlas::default(),
                        slot,
                        started: tick as f32 / 64.,
                        updated: now,
                        seed_centres: Vec::new(),
                    });
                }
                let render = v.render.as_ref().expect("initialized projection");
                ensure!(
                    render.started == tick as f32 / 64. && origin.iter().all(|x| x.is_finite()),
                    "canonical smoke lifetime changed without removal"
                );
            }
        }
        for key in dirty {
            if let Some(volume) = self.volumes.get_mut(&key) {
                let previous = volume.consumed;
                volume.advance(|density| changed(key, density))?;
                if volume.consumed != previous {
                    if let Some(render) = &mut volume.render {
                        // Canonical upload at recorded arrival, not native client draw time.
                        render.updated = frame.net_tick as f32 / 64.;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn removal_deltas_do_not_recreate_a_deleted_cloud() {
        let fields: Vec<_> = ["$present", "m_bDidSmokeEffect"].into_iter().map(|name| compact::Field {
            entity: 1, serial: 1, class: "CSmokeGrenadeProjectile".into(), name: name.into(),
        }).collect();
        let mut timeline = Timeline::default();
        let values = BTreeMap::from([(0,vec![0,1]),(1,vec![0,1])]);
        timeline.update(&compact::Frame {tick:1,net_tick:1,fields:&fields,values:&values,changed:&[0,1]}, |_,_| Ok(())).unwrap();
        assert_eq!(timeline.volumes().count(),1);
        timeline.update(&compact::Frame {tick:2,net_tick:2,fields:&fields,values:&BTreeMap::new(),changed:&[0,1]}, |_,_| Ok(())).unwrap();
        assert_eq!(timeline.volumes().count(),0);
    }
    #[test]
    fn estimated_samples_require_stable_complete_contiguous_smoke() {
        let mut density = Density::new([0.; 3]).unwrap();
        density.step(0, &[0, 0, 0]).unwrap();
        let mut timeline = Timeline {
            before_update: true,
            packet_net_tick: Some(200),
            clock: Clock {
                paused_ticks: Some(0),
                paused: Some(false),
                pause_start: None,
            },
            ..Default::default()
        };
        timeline.volumes.insert(
            (1, 1),
            Volume {
                present: true,
                did_effect: Some(true),
                effect_tick: Some(64),
                origin: Some([0.; 3]),
                density: Some(density),
                size: Some(0),
                consumed: 0,
                ..Default::default()
            },
        );
        assert_eq!(timeline.stable_volumes_at_fire(200).unwrap().len(), 1);
        timeline.volumes.get_mut(&(1, 1)).unwrap().size = Some(10);
        assert!(timeline.stable_volumes_at_fire(200).is_none());
        timeline.volumes.get_mut(&(1, 1)).unwrap().size = Some(0);
        timeline.packet_net_tick = Some(100);
        assert!(timeline.stable_volumes_at_fire(100).is_none());
        timeline.packet_net_tick = Some(1200);
        assert!(timeline.stable_volumes_at_fire(1200).is_none());
        timeline.packet_net_tick = Some(200);
        timeline.before_update = false;
        assert!(timeline.stable_volumes_at_fire(200).is_none());
    }
    #[test]
    fn absent_smoke_is_certified_only_in_contiguous_fire_phase() {
        let mut timeline = Timeline::default();
        assert!(!timeline.has_no_density_at_fire());
        timeline.before_update = true;
        assert!(timeline.has_no_density_at_fire());
        timeline.volumes.insert(
            (1, 1),
            Volume {
                present: true,
                ..Volume::default()
            },
        );
        assert!(!timeline.has_no_density_at_fire());
        let volume = timeline.volumes.get_mut(&(1, 1)).unwrap();
        volume.did_effect = Some(false);
        assert!(timeline.has_no_density_at_fire());
        timeline.before_update = false;
        assert!(!timeline.has_no_density_at_fire());
    }
    #[test]
    fn recorded_empty_activation_is_distinct_from_missing_journal_size() {
        let mut volume = Volume {
            present: true,
            did_effect: Some(true),
            effect_tick: Some(64),
            origin: Some([0.; 3]),
            ..Volume::default()
        };
        volume.advance(|_| Ok(())).unwrap();
        assert!(volume.density().is_none());
        volume.size = Some(0);
        volume.advance(|_| Ok(())).unwrap();
        assert!(volume.empty_initial());
        assert!(volume.density().is_none());
    }

    #[test]
    fn native_clock_interval_contains_subticks_and_preserves_pause_domain() {
        let mut clock = Clock {
            paused_ticks: Some(17003),
            pause_start: None,
            paused: Some(false),
        };
        let tick = 1_500_001;
        let bounds = clock.interval(tick).unwrap();
        for part in 0..1024 {
            let raw = (tick + 17003) as f32 * (1. / 64.) + (part as f32 / 1024.) * (1. / 64.);
            let native = raw - 17003_f32 * (1. / 64.);
            assert!(bounds[0] <= native && native <= bounds[1]);
        }
        clock.paused = Some(true);
        clock.pause_start = Some(1_517_004);
        assert_eq!(clock.interval(tick), Some([tick as f32 / 64.; 2]));
        clock.paused_ticks = None;
        assert!(clock.interval(tick).is_none());
    }

    #[test]
    fn fire_reads_previous_journal_and_gaps_are_not_certified() {
        let mut volume = Volume {
            present: true,
            did_effect: Some(true),
            effect_tick: Some(64),
            origin: Some([0.; 3]),
            size: Some(7),
            slots: [0, 0, 3, 0, 0, 0, 0, 1, 0, 3, 0, 0, 0, 0]
                .into_iter()
                .map(Some)
                .collect(),
            ..Volume::default()
        };
        volume.advance(|_| Ok(())).unwrap();
        let mut timeline = Timeline {
            last_net_tick: Some(100),
            clock: Clock {
                paused_ticks: Some(0),
                paused: Some(false),
                pause_start: None,
            },
            ..Timeline::default()
        };
        timeline.volumes.insert((7, 1), volume);
        let fields = vec![compact::Field {
            entity: 7,
            serial: 1,
            class: "CSmokeGrenadeProjectile".into(),
            name: "m_nVoxelFrameDataSize".into(),
        }];
        let values = BTreeMap::from([(0, vec![1, 14, 0, 0, 0])]);
        let frame = compact::Frame {
            tick: 101,
            net_tick: 101,
            fields: &fields,
            values: &values,
            changed: &[0],
        };
        timeline.begin_packet(&frame);
        assert_eq!(
            timeline.volumes[&(7, 1)].density().unwrap().sequence(),
            Some(0)
        );
        assert_eq!(
            timeline.crosses_at_fire(101, [0.; 3], [1.; 3]).unwrap(),
            Some(false)
        );
        assert_eq!(
            timeline.crosses_at_fire(99, [0.; 3], [1.; 3]).unwrap(),
            None
        );
        assert_eq!(
            timeline.crosses_at_fire(102, [0.; 3], [1.; 3]).unwrap(),
            None
        );
        timeline.update(&frame, |_, _| Ok(())).unwrap();
        assert_eq!(
            timeline.volumes[&(7, 1)].density().unwrap().sequence(),
            Some(1)
        );
        assert_eq!(
            timeline.crosses_at_fire(101, [0.; 3], [1.; 3]).unwrap(),
            None
        );
        let gap = compact::Frame {
            tick: 103,
            net_tick: 103,
            ..frame
        };
        timeline.begin_packet(&gap);
        assert_eq!(
            timeline.crosses_at_fire(103, [0.; 3], [1.; 3]).unwrap(),
            None
        );
    }

    #[test]
    fn incomplete_and_rewritten_journals_never_expose_stale_density() {
        let mut volume = Volume {
            present: true,
            origin: Some([0.; 3]),
            size: Some(7),
            ..Volume::default()
        };
        volume.slots = vec![Some(0), Some(0), Some(3), Some(0), Some(0), Some(0), None];
        let mut updates = 0;
        volume
            .advance(|_| {
                updates += 1;
                Ok(())
            })
            .unwrap();
        assert!(volume.density().is_none());
        volume.slots[6] = Some(0);
        volume
            .advance(|_| {
                updates += 1;
                Ok(())
            })
            .unwrap();
        volume
            .advance(|_| {
                updates += 1;
                Ok(())
            })
            .unwrap();
        assert_eq!(updates, 1);
        assert_eq!(volume.density().unwrap().sequence(), Some(0));
        volume.size = Some(14);
        assert!(volume.density().is_none());
        volume.slots.extend([
            Some(2),
            Some(0),
            Some(3),
            Some(0),
            Some(0),
            Some(0),
            Some(0),
        ]);
        volume.advance(|_| Ok(())).unwrap();
        assert!(volume.error().is_some());
        assert!(volume.density().is_none());
        let mut timeline = Timeline::default();
        timeline.volumes.insert((7, 1), volume);
        let fields = vec![compact::Field {
            entity: 7,
            serial: 1,
            class: "CSmokeGrenadeProjectile".into(),
            name: "$present".into(),
        }];
        timeline
            .update(
                &compact::Frame {
                    tick: 10,
                    net_tick: 10,
                    fields: &fields,
                    values: &BTreeMap::new(),
                    changed: &[0],
                },
                |_, _| Ok(()),
            )
            .unwrap();
        assert_eq!(timeline.volumes().count(), 0);
    }
    #[test]
    fn canonical_projection_batches_history_slots_missing_data_and_reset() {
        fn volume() -> Volume {
            let bytes = [0, 0, 12, 0, 0, 1, 1, 16, 16, 16, 0, 0, 0, 0, 0, 0];
            Volume {
                present: true,
                did_effect: Some(true),
                origin: Some([0.; 3]),
                effect_tick: Some(64),
                size: Some(bytes.len()),
                slots: bytes.into_iter().map(Some).collect(),
                ..Volume::default()
            }
        }
        let mut timeline = Timeline::with_render_state();
        timeline.volumes.insert((9, 1), volume());
        timeline.volumes.insert((2, 1), volume());
        let fields: Vec<_> = [9, 2]
            .map(|entity| compact::Field {
                entity,
                serial: 1,
                class: "CSmokeGrenadeProjectile".into(),
                name: "$present".into(),
            })
            .into();
        let values = BTreeMap::from([(0, vec![0, 1]), (1, vec![0, 1])]);
        let mut updates = 0;
        for tick in [64, 65] {
            timeline
                .update(
                    &compact::Frame {
                        tick,
                        net_tick: tick as u32,
                        fields: &fields,
                        values: &values,
                        changed: &[0, 1],
                    },
                    |_, _| {
                        updates += 1;
                        Ok(())
                    },
                )
                .unwrap();
        }
        assert_eq!(updates, 2); // One step per cloud, no repeat on unchanged journal.
        assert_eq!(timeline.volumes[&(9, 1)].render.as_ref().unwrap().slot, 0);
        assert_eq!(timeline.volumes[&(2, 1)].render.as_ref().unwrap().slot, 1);
        assert_eq!(
            timeline.registered_smokes().unwrap()[0].seed_centres,
            &[[10.; 3]]
        );
        let old = timeline.volumes[&(9, 1)]
            .render
            .as_ref()
            .unwrap()
            .atlas
            .cells()
            .to_vec();
        let v = timeline.volumes.get_mut(&(9, 1)).unwrap();
        v.slots
            .extend([1, 0, 12, 0, 0, 1, 1, 17, 16, 16, 0, 0, 0, 0, 0, 0].map(Some));
        v.size = Some(32);
        assert!(timeline.render_clouds(66. / 64.).is_err());
        timeline
            .update(
                &compact::Frame {
                    tick: 66,
                    net_tick: 66,
                    fields: &fields,
                    values: &values,
                    changed: &[0],
                },
                |_, _| Ok(()),
            )
            .unwrap();
        let r = timeline.volumes[&(9, 1)].render.as_ref().unwrap();
        assert_eq!(r.seed_centres, vec![[10.; 3]]); // Freeze the initial seed list.
        assert_eq!(r.updated, 66. / 64.);
        let clouds = timeline.render_clouds(66. / 64.).unwrap();
        assert_eq!(clouds[0].parameters.interpolation, 0.);
        assert!(r
            .atlas
            .cells()
            .iter()
            .zip(old)
            .any(|(new, old)| new[0] == old[1] && new[1] != old[1]));
        timeline
            .update(
                &compact::Frame {
                    tick: 67,
                    net_tick: 67,
                    fields: &fields,
                    values: &BTreeMap::new(),
                    changed: &[0],
                },
                |_, _| Ok(()),
            )
            .unwrap();
        assert_eq!(timeline.render_clouds(67. / 64.).unwrap()[0].slot, 1);
        assert_eq!(
            timeline.render.as_ref().unwrap().free_slots.last(),
            Some(&0)
        );
        timeline
            .render_effects_mut()
            .unwrap()
            .push_he(super::super::effects::HeRecord {
                identity: 1,
                position: [0.; 3],
                time: 1.,
                mask: 2,
            })
            .unwrap();
        timeline.reset();
        assert_eq!(timeline.volumes().count(), 0);
        assert_eq!(timeline.render_effects().unwrap().he_records().count(), 0);
        assert_eq!(timeline.render.as_ref().unwrap().free_slots.len(), 16);
    }
}
