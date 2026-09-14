//! Raw AnimGraph2 clip samples, before graph tasks and world transforms.
//! Compression follows ValveResourceFormat's AnimationClip.ReadFrame; no DMX exporter corrections.
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq)]
pub struct Transform {
    pub position: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: f32,
}

#[derive(Debug)]
pub struct Clip {
    pub skeleton: String,
    pub num_frames: usize,
    pub duration: f32,
    pub is_additive: bool,
    frames: Vec<std::sync::OnceLock<std::result::Result<Vec<Transform>, String>>>,
    tracks: Vec<Track>,
    samples: Vec<u16>,
    offsets: Vec<usize>,
    model_space_chain: Vec<ChainLink>,
    model_space_indices: Vec<usize>,
}

#[derive(Deserialize)]
struct Source {
    #[serde(rename = "m_skeleton")]
    skeleton: String,
    #[serde(rename = "m_nNumFrames")]
    num_frames: usize,
    #[serde(rename = "m_flDuration")]
    duration: f32,
    #[serde(rename = "m_bIsAdditive")]
    is_additive: bool,
    #[serde(rename = "m_trackCompressionSettings")]
    tracks: Vec<Track>,
    #[serde(rename = "m_compressedPoseData")]
    data: Vec<u8>,
    #[serde(rename = "m_compressedPoseOffsets")]
    offsets: Vec<usize>,
    #[serde(rename = "m_modelSpaceBoneSamplingIndices", default)]
    model_space_indices: Vec<usize>,
    #[serde(rename = "m_modelSpaceSamplingChain", default)]
    model_space_chain: Vec<ChainLink>,
}

#[derive(Debug, Deserialize)]
struct ChainLink {
    #[serde(rename = "m_nBoneIdx")]
    bone: usize,
    #[serde(rename = "m_nParentChainLinkIdx")]
    parent: i32,
}

#[derive(Clone, Debug, Deserialize)]
struct Range {
    #[serde(rename = "m_flRangeStart")]
    start: f32,
    #[serde(rename = "m_flRangeLength")]
    length: f32,
}

#[derive(Clone, Debug, Deserialize)]
struct Track {
    #[serde(rename = "m_translationRangeX")]
    x: Range,
    #[serde(rename = "m_translationRangeY")]
    y: Range,
    #[serde(rename = "m_translationRangeZ")]
    z: Range,
    #[serde(rename = "m_scaleRange")]
    scale: Range,
    #[serde(rename = "m_constantRotation")]
    rotation: [f32; 4],
    #[serde(rename = "m_bIsRotationStatic")]
    rotation_static: bool,
    #[serde(rename = "m_bIsTranslationStatic")]
    translation_static: bool,
    #[serde(rename = "m_bIsScaleStatic")]
    scale_static: bool,
}

impl Clip {
    pub fn from_kv3(text: &str) -> Result<Self> {
        let source: Source = serde_json::from_value(super::kv3_text::parse(text)?)
            .context("Invalid animation clip fields")?;
        ensure!(!source.skeleton.is_empty(), "Missing animation skeleton");
        ensure!(
            (1..=100_000).contains(&source.num_frames),
            "Invalid clip frame count"
        );
        ensure!(
            (1..=1024).contains(&source.tracks.len()),
            "Invalid clip bone count"
        );
        ensure!(
            source.duration.is_finite() && source.duration >= 0.,
            "Invalid clip duration"
        );
        ensure!(
            source.offsets.len() == source.num_frames,
            "Clip frame offsets are missing"
        );
        ensure!(
            source.data.len().is_multiple_of(2),
            "Truncated clip sample word"
        );
        let words_per_frame: usize = source
            .tracks
            .iter()
            .map(|t| {
                usize::from(!t.rotation_static) * 3
                    + usize::from(!t.translation_static) * 3
                    + usize::from(!t.scale_static)
            })
            .sum();
        for track in &source.tracks {
            for range in [&track.x, &track.y, &track.z, &track.scale] {
                ensure!(
                    range.start.is_finite() && range.length.is_finite() && range.length >= 0.,
                    "Invalid clip compression range"
                );
            }
            if track.rotation_static {
                validate_rotation(track.rotation)?;
            }
        }
        let sample_count = source.data.len() / 2;
        for (index, &offset) in source.offsets.iter().enumerate() {
            let end = offset
                .checked_add(words_per_frame)
                .context("Clip offset overflow")?;
            ensure!(
                end <= sample_count,
                "Truncated compressed clip frame {index}"
            );
            if let Some(&next) = source.offsets.get(index + 1) {
                ensure!(next >= end, "Overlapping compressed clip frames");
            }
        }
        ensure!(
            source.model_space_chain.len() <= source.tracks.len(),
            "Too many model-space links"
        );
        let mut chain_bones = std::collections::HashSet::new();
        for (index, link) in source.model_space_chain.iter().enumerate() {
            ensure!(
                link.bone < source.tracks.len() && chain_bones.insert(link.bone),
                "Invalid or duplicate model-space bone"
            );
            ensure!(
                if link.bone == 0 {
                    link.parent == -1
                } else {
                    link.parent >= 0 && (link.parent as usize) < index
                },
                "Invalid model-space parent order"
            );
        }
        let mut selected = std::collections::HashSet::new();
        for &index in &source.model_space_indices {
            ensure!(
                index < source.model_space_chain.len() && selected.insert(index),
                "Invalid model-space sampling index"
            );
        }
        Ok(Self {
            skeleton: source.skeleton,
            num_frames: source.num_frames,
            duration: source.duration,
            is_additive: source.is_additive,
            frames: (0..source.num_frames)
                .map(|_| std::sync::OnceLock::new())
                .collect(),
            tracks: source.tracks,
            samples: source
                .data
                .chunks_exact(2)
                .map(|b| u16::from_le_bytes([b[0], b[1]]))
                .collect(),
            offsets: source.offsets,
            model_space_chain: source.model_space_chain,
            model_space_indices: source.model_space_indices,
        })
    }

    pub fn frame(&self, index: usize) -> Result<Vec<Transform>> {
        let cached = self.frames.get(index).context("Clip frame outside range")?;
        cached
            .get_or_init(|| self.decode_frame(index).map_err(|e| e.to_string()))
            .clone()
            .map_err(anyhow::Error::msg)
    }

    fn decode_frame(&self, index: usize) -> Result<Vec<Transform>> {
        let mut at = *self
            .offsets
            .get(index)
            .context("Clip frame outside range")?;
        let mut result = Vec::with_capacity(self.tracks.len());
        for track in &self.tracks {
            let mut transform = Transform {
                position: [track.x.start, track.y.start, track.z.start],
                rotation: track.rotation,
                scale: track.scale.start,
            };
            if !track.rotation_static {
                transform.rotation = decode_rotation([
                    self.samples[at],
                    self.samples[at + 1],
                    self.samples[at + 2],
                ])?;
                at += 3;
            }
            if !track.translation_static {
                for (axis, range) in [&track.x, &track.y, &track.z].iter().enumerate() {
                    transform.position[axis] = decode_float(self.samples[at], range);
                    at += 1;
                }
            }
            if !track.scale_static {
                transform.scale = decode_float(self.samples[at], &track.scale);
                at += 1;
            }
            ensure!(
                transform.position.iter().all(|v| v.is_finite()) && transform.scale.is_finite(),
                "Nonfinite decoded clip transform"
            );
            result.push(transform);
        }
        Ok(result)
    }

    /// The serialized sample task supplies normalized time, including the terminal frame at 1.
    pub fn sample(&self, time: f32) -> Result<Vec<Transform>> {
        ensure!(
            time.is_finite() && (0.0..=1.0).contains(&time),
            "Invalid normalized clip time"
        );
        let frame = time * (self.num_frames - 1) as f32;
        let lower = frame.floor() as usize;
        let fraction = frame - lower as f32;
        let mut result = self.frame(lower)?;
        if fraction == 0. || lower + 1 == self.num_frames {
            return Ok(result);
        }
        let upper = self.frame(lower + 1)?;
        let source_model = self.model_transforms(&result);
        let target_model = self.model_transforms(&upper);
        for (a, b) in result.iter_mut().zip(&upper) {
            *a = Transform::interpolate(*a, *b, fraction)?;
        }
        if !self.model_space_indices.is_empty() {
            let blended_model = self.model_transforms(&result);
            // The engine converts all selected bones against the same pre-correction parent pose.
            for &index in &self.model_space_indices {
                let link = &self.model_space_chain[index];
                let model =
                    Transform::interpolate(source_model[index], target_model[index], fraction)?;
                result[link.bone] = if link.parent < 0 {
                    model
                } else {
                    Transform::compose(blended_model[link.parent as usize].inverse()?, model)
                };
            }
        }
        Ok(result)
    }

    fn model_transforms(&self, local: &[Transform]) -> Vec<Transform> {
        let mut result = Vec::with_capacity(self.model_space_chain.len());
        for link in &self.model_space_chain {
            result.push(if link.bone == 0 {
                Transform::IDENTITY
            } else {
                Transform::compose(result[link.parent as usize], local[link.bone])
            });
        }
        result
    }
}

impl Transform {
    pub const IDENTITY: Self = Self {
        position: [0.; 3],
        rotation: [0., 0., 0., 1.],
        scale: 1.,
    };

    pub fn compose(parent: Self, local: Self) -> Self {
        let rotated = rotate(parent.rotation, local.position.map(|v| v * parent.scale));
        Self {
            position: std::array::from_fn(|i| parent.position[i] + rotated[i]),
            rotation: quaternion_multiply(parent.rotation, local.rotation),
            scale: parent.scale * local.scale,
        }
    }

    pub fn inverse(self) -> Result<Self> {
        ensure!(
            self.scale.is_finite() && self.scale != 0.,
            "Cannot invert zero/nonfinite pose scale"
        );
        validate_rotation(self.rotation)?;
        let rotation = [
            -self.rotation[0],
            -self.rotation[1],
            -self.rotation[2],
            self.rotation[3],
        ];
        Ok(Self {
            position: rotate(rotation, self.position.map(|v| -v / self.scale)),
            rotation,
            scale: 1. / self.scale,
        })
    }

    pub fn interpolate(a: Self, b: Self, time: f32) -> Result<Self> {
        ensure!(
            time.is_finite() && (0. ..=1.).contains(&time),
            "Invalid interpolation fraction"
        );
        let transform = Self {
            position: std::array::from_fn(|i| {
                a.position[i] + (b.position[i] - a.position[i]) * time
            }),
            rotation: slerp(a.rotation, b.rotation, time)?,
            scale: a.scale + (b.scale - a.scale) * time,
        };
        ensure!(
            transform.position.iter().all(|v| v.is_finite()) && transform.scale.is_finite(),
            "Nonfinite pose interpolation"
        );
        Ok(transform)
    }
}

fn quaternion_multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let t = [
        2. * (q[1] * v[2] - q[2] * v[1]),
        2. * (q[2] * v[0] - q[0] * v[2]),
        2. * (q[0] * v[1] - q[1] * v[0]),
    ];
    [
        v[0] + q[3] * t[0] + q[1] * t[2] - q[2] * t[1],
        v[1] + q[3] * t[1] + q[2] * t[0] - q[0] * t[2],
        v[2] + q[3] * t[2] + q[0] * t[1] - q[1] * t[0],
    ]
}

fn decode_float(sample: u16, range: &Range) -> f32 {
    (f32::from(sample) / 65535.) * range.length + range.start
}

fn validate_rotation(q: [f32; 4]) -> Result<()> {
    let norm: f32 = q.iter().map(|v| v * v).sum();
    ensure!(
        norm.is_finite() && (norm - 1.).abs() <= 0.002,
        "Invalid clip quaternion"
    );
    Ok(())
}

pub(crate) fn decode_rotation(words: [u16; 3]) -> Result<[f32; 4]> {
    let minimum = -std::f32::consts::FRAC_1_SQRT_2;
    let multiplier = (-2. * minimum) / 32767.;
    let values = [words[0] & 0x7fff, words[1] & 0x7fff, words[2]]
        .map(|v| f32::from(v).mul_add(multiplier, minimum));
    let sum: f32 = values.iter().map(|v| v * v).sum();
    ensure!(sum <= 1.0001, "Invalid compressed clip quaternion");
    let missing = (1. - sum).max(0.).sqrt();
    let largest = ((words[0] >> 14) & 2) | (words[1] >> 15);
    let q = match largest {
        0 => [missing, values[0], values[1], values[2]],
        1 => [values[0], missing, values[1], values[2]],
        2 => [values[0], values[1], missing, values[2]],
        _ => [values[0], values[1], values[2], missing],
    };
    validate_rotation(q)?;
    Ok(q)
}

fn slerp(a: [f32; 4], mut b: [f32; 4], t: f32) -> Result<[f32; 4]> {
    let mut dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    if dot < 0. {
        b = b.map(|v| -v);
        dot = -dot;
    }
    let (wa, wb) = if dot > 0.9995 {
        (1. - t, t)
    } else {
        let angle = dot.clamp(-1., 1.).acos();
        let denominator = angle.sin();
        (
            ((1. - t) * angle).sin() / denominator,
            (t * angle).sin() / denominator,
        )
    };
    let mut result = std::array::from_fn(|i| wa * a[i] + wb * b[i]);
    let norm = result.iter().map(|v| v * v).sum::<f32>().sqrt();
    ensure!(
        norm.is_finite() && norm > 0.,
        "Invalid interpolated clip quaternion"
    );
    for value in &mut result {
        *value /= norm;
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(rotation_static: bool, data: &str) -> String {
        format!(
            r#"{{
            m_skeleton=resource:"test.vnmskel" m_nNumFrames=2 m_flDuration=1 m_bIsAdditive=true
            m_compressedPoseData=#[ {data} ] m_compressedPoseOffsets=[0,0]
            m_trackCompressionSettings=[{{
                m_translationRangeX={{m_flRangeStart=1 m_flRangeLength=1}}
                m_translationRangeY={{m_flRangeStart=2 m_flRangeLength=1}}
                m_translationRangeZ={{m_flRangeStart=3 m_flRangeLength=1}}
                m_scaleRange={{m_flRangeStart=0 m_flRangeLength=1}}
                m_constantRotation=[0,0,0,1] m_bIsRotationStatic={rotation_static}
                m_bIsTranslationStatic=true m_bIsScaleStatic=true
            }}]
        }}"#
        )
    }

    #[test]
    fn static_additive_and_truncated_clip() {
        let clip = Clip::from_kv3(&fixture(true, "")).unwrap();
        assert!(clip.is_additive);
        assert_eq!(
            clip.sample(0.5).unwrap()[0],
            Transform {
                position: [1., 2., 3.],
                rotation: [0., 0., 0., 1.],
                scale: 0.
            }
        );
        assert!(clip.sample(f32::NAN).is_err());
        assert!(clip.frame(2).is_err());
        assert!(Clip::from_kv3(&fixture(false, "")).is_err());
        assert!(Clip::from_kv3(&fixture(true, "01")).is_err());
        let mut model_space = fixture(true, "");
        model_space.insert_str(1, "m_modelSpaceBoneSamplingIndices=[0] ");
        assert!(Clip::from_kv3(&model_space).is_err());
    }

    #[test]
    fn model_space_interpolation_uses_original_parent_chain() {
        let mut track = Clip::from_kv3(&fixture(true, "")).unwrap().tracks.remove(0);
        track.x.start = 0.;
        track.y.start = 0.;
        track.z.start = 0.;
        track.scale.start = 1.;
        let mut rotating = track.clone();
        rotating.rotation_static = false;
        let mut child = track.clone();
        child.x.start = 1.;
        let mut clip = Clip {
            skeleton: "synthetic".into(),
            num_frames: 2,
            duration: 1.,
            is_additive: false,
            frames: (0..2).map(|_| std::sync::OnceLock::new()).collect(),
            tracks: vec![track, rotating, child],
            samples: vec![0xbfff, 0xbfff, 0x3fff, 0xbfff, 0x3fff, 0x3fff],
            offsets: vec![0, 3],
            model_space_chain: vec![
                ChainLink {
                    bone: 0,
                    parent: -1,
                },
                ChainLink { bone: 1, parent: 0 },
                ChainLink { bone: 2, parent: 1 },
            ],
            model_space_indices: vec![2],
        };
        let sampled = clip.sample(0.5).unwrap();
        let model = Transform::compose(sampled[1], sampled[2]);
        assert!(model.position.iter().all(|v| v.abs() < 0.0001));
        clip.model_space_indices.clear();
        let local = clip.sample(0.5).unwrap();
        let local_world = Transform::compose(local[1], local[2]);
        assert!(local_world.position[1].abs() > 0.99);
        assert!(Transform {
            scale: 0.,
            ..Transform::IDENTITY
        }
        .inverse()
        .is_err());
    }

    #[test]
    fn quaternion_decode_and_interpolation_keep_rotation() {
        let q = decode_rotation([0xbfff, 0xbfff, 0x3fff]).unwrap();
        assert!(q[3] > 0.9999);
        let a = [0., 0., 0., 1.];
        assert_eq!(slerp(a, [0., 0., 0., -1.], 0.5).unwrap(), a);
        let mid = slerp(a, [0., 0., 1., 0.], 0.5).unwrap();
        assert!((mid[2] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!(decode_rotation([0x7fff, 0x7fff, 0xffff]).is_err());
    }
}
