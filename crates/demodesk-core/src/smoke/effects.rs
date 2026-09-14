//! Native visual-disturbance rings. These do not mutate server CPU smoke density.
//! Geometry, registration, view offsets and the clock are qualified by the caller.
use anyhow::{ensure, Result};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HeRecord {
    pub identity: u64,
    pub position: [f32; 3],
    pub time: f32,
    pub mask: u32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulletRecord {
    /// Already adjusted for the qualified native view mode.
    pub start: [f32; 3],
    pub end: [f32; 3],
    pub time: f32,
    pub flag: u8,
    pub width: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PackedHe {
    pub position_time: [f32; 4],
    /// Numeric u32-to-f32 conversion, not a bit reinterpretation.
    pub mask: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PackedBullet {
    pub start_age: [f32; 4],
    pub end_flag: [f32; 4],
    pub width: [f32; 4],
}
#[derive(Clone, Debug, Default)]
pub struct EffectFrame {
    pub now: f32,
    pub he: [PackedHe; 5],
    pub he_count: usize,
    pub bullets: [PackedBullet; 16],
    pub bullet_count: usize,
}

#[derive(Clone, Debug)]
struct Ring<T: Copy, const N: usize> {
    slots: [Option<T>; N],
    head: usize,
    len: usize,
}
impl<T: Copy, const N: usize> Default for Ring<T, N> {
    fn default() -> Self {
        Self {
            slots: [None; N],
            head: 0,
            len: 0,
        }
    }
}
impl<T: Copy, const N: usize> Ring<T, N> {
    fn push(&mut self, value: T) {
        self.slots[(self.head + self.len) % N] = Some(value);
        if self.len == N {
            self.head = (self.head + 1) % N;
        } else {
            self.len += 1;
        }
    }
    fn iter(&self) -> impl Iterator<Item = &T> {
        (0..self.len).filter_map(|i| self.slots[(self.head + i) % N].as_ref())
    }
    fn remove_first(&mut self, predicate: impl Fn(&T) -> bool) -> bool {
        let found = self.iter().position(predicate);
        let Some(i) = found else {
            return false;
        };
        let last = (self.head + self.len - 1) % N;
        // CCDDE0 replaces the removed entry with the last logical entry. It does
        // not shift the intervening entries or advance the head.
        self.slots[(self.head + i) % N] = self.slots[last];
        self.slots[last] = None;
        self.len -= 1;
        true
    }
}
#[derive(Clone, Debug, Default)]
pub struct Effects {
    he: Ring<HeRecord, 5>,
    bullets: Ring<BulletRecord, 16>,
}
impl Effects {
    /// Insert only an independently qualified HE mask. Zero means no insertion.
    pub fn push_he(&mut self, record: HeRecord) -> Result<bool> {
        ensure!(
            record.time.is_finite() && record.position.iter().all(|v| v.is_finite()),
            "nonfinite HE effect"
        );
        if record.mask == 0 {
            return Ok(false);
        }
        self.he.push(record);
        Ok(true)
    }
    pub fn remove_he(&mut self, identity: u64) -> bool {
        self.he.remove_first(|record| record.identity == identity)
    }
    /// The caller has already established native registration and final endpoints.
    pub fn push_bullet(&mut self, record: BulletRecord) -> Result<()> {
        ensure!(
            record.time.is_finite()
                && record.width.is_finite()
                && record
                    .start
                    .iter()
                    .chain(&record.end)
                    .all(|v| v.is_finite()),
            "nonfinite bullet effect"
        );
        self.bullets.push(record);
        Ok(())
    }
    pub fn he_records(&self) -> impl Iterator<Item = &HeRecord> {
        self.he.iter()
    }
    pub fn bullet_records(&self) -> impl Iterator<Item = &BulletRecord> {
        self.bullets.iter()
    }
    /// Pack at the qualified native visual clock; expiry does not remove records.
    pub fn pack(&self, now: f32) -> Result<EffectFrame> {
        ensure!(now.is_finite(), "nonfinite effects clock");
        let mut frame = EffectFrame {
            now,
            ..EffectFrame::default()
        };
        for record in self.he.iter() {
            let age = now - record.time;
            if !(0. ..5.).contains(&age) {
                continue;
            }
            frame.he[frame.he_count] = PackedHe {
                position_time: [
                    record.position[0],
                    record.position[1],
                    record.position[2],
                    record.time,
                ],
                mask: record.mask as f32,
            };
            frame.he_count += 1;
        }
        for record in self.bullets.iter() {
            let age = now - record.time;
            if !(0. ..1.).contains(&age) {
                continue;
            }
            // CCED90..CCEF62 uses separate scalar multiplies/adds, extrapolates
            // beyond the endpoint and adds the same factor to both Z coordinates.
            let factor = age * 10.;
            let other = 1. - factor;
            let mut end: [f32; 3] =
                std::array::from_fn(|i| record.end[i] * factor + record.start[i] * other);
            end[2] += factor;
            ensure!(
                end.iter().all(|v| v.is_finite()) && (record.start[2] + factor).is_finite(),
                "effect packing overflow"
            );
            frame.bullets[frame.bullet_count] = PackedBullet {
                start_age: [
                    record.start[0],
                    record.start[1],
                    record.start[2] + factor,
                    age,
                ],
                end_flag: [end[0], end[1], end[2], f32::from(record.flag != 0)],
                width: [record.width, 0., 0., 0.],
            };
            frame.bullet_count += 1;
        }
        Ok(frame)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_capacity_swap_removal_and_age_packing() {
        let mut effects = Effects::default();
        for identity in 0..6 {
            effects
                .push_he(HeRecord {
                    identity,
                    position: [1., 2., 3.],
                    time: 10.,
                    mask: 1 << identity,
                })
                .unwrap();
        }
        assert_eq!(
            effects.he_records().map(|r| r.identity).collect::<Vec<_>>(),
            [1, 2, 3, 4, 5]
        );
        assert!(effects.remove_he(2));
        assert_eq!(
            effects.he_records().map(|r| r.identity).collect::<Vec<_>>(),
            [1, 5, 3, 4]
        );
        assert!(!effects.remove_he(99));
        assert!(!effects
            .push_he(HeRecord {
                identity: 99,
                position: [0.; 3],
                time: 10.,
                mask: 0
            })
            .unwrap());
        for i in 0..17 {
            effects
                .push_bullet(BulletRecord {
                    start: [i as f32, 2., 3.],
                    end: [20., 4., 5.],
                    time: 10.,
                    flag: 2,
                    width: 0.25,
                })
                .unwrap();
        }
        assert_eq!(effects.bullet_records().next().unwrap().start[0], 1.);
        let frame = effects.pack(10.5).unwrap();
        assert_eq!((frame.he_count, frame.bullet_count), (4, 16));
        assert_eq!(frame.bullets[0].start_age, [1., 2., 8., 0.5]);
        assert_eq!(frame.bullets[0].end_flag, [96., 12., 18., 1.]);
        assert_eq!(frame.bullets[0].width, [0.25, 0., 0., 0.]);
        assert_eq!(effects.pack(9.).unwrap().bullet_count, 0);
        assert_eq!(effects.pack(11.).unwrap().bullet_count, 0);
        assert_eq!(effects.pack(15.).unwrap().he_count, 0);
        assert_eq!(effects.pack(10.).unwrap().bullet_count, 16);
        assert_eq!(effects.he_records().count(), 4);
    }
}
