//! CS2 AimCS spine rotations for the client validated by animation_pose.
//! Shared torso, weapon and arm reconstruction from recorded task parameters.
use super::animation_clip::Transform;
use super::animation_pose::{Pose, Skeleton};
use anyhow::{ensure, Context, Result};

/// Updates the six spine/neck/head local transforms. The caller must validate the
/// client write-set before use. Mode 3 applies another head task afterward, so its
/// head must remain unknown until that task has been evaluated.
pub fn apply_spine(
    skeleton: &Skeleton,
    pose: &mut Pose,
    normalized16: [u16; 2],
    normalized8: [u8; 4],
    mode: u8,
) -> Result<bool> {
    ensure!(
        mode <= 5 && !pose.is_additive,
        "Unsupported AimCS pose or mode"
    );
    let mut chain = Vec::with_capacity(7);
    for name in [
        "pelvis", "spine_0", "spine_1", "spine_2", "spine_3", "neck_0", "head_0",
    ] {
        let index = skeleton
            .names
            .iter()
            .position(|n| n == name)
            .context("Missing AimCS bone")?;
        if let Some(&parent) = chain.last() {
            ensure!(
                skeleton.parents[index] == Some(parent),
                "Unsupported AimCS hierarchy"
            );
        }
        chain.push(index);
    }
    ensure!(
        pose.local.len() == skeleton.names.len(),
        "Incomplete AimCS pose"
    );
    let pelvis = pose
        .model
        .get(chain[0])
        .copied()
        .flatten()
        .context("Unknown AimCS pelvis")?;
    let mut local: Vec<_> = chain[1..]
        .iter()
        .map(|&i| pose.local[i].context("Unknown AimCS input bone"))
        .collect::<Result<_>>()?;
    let mut model = Vec::with_capacity(7);
    model.push(pelvis);
    for &transform in &local {
        model.push(Transform::compose(
            *model.last().expect("pelvis exists"),
            transform,
        ));
    }
    let yaw = normalized16[0] as f32 / 65535. * 180. - 90.;
    let pitch = normalized16[1] as f32 / 65535. * 180. - 90.;
    let restriction = normalized8[1] as f32 / 255. * 40.;
    let pitch = if restriction > 0. {
        let low = restriction - 90.;
        let high = 90. - restriction;
        ((pitch + 90.) / 180.).clamp(0., 1.) * (high - low) + low
    } else {
        pitch
    };
    let crouch = normalized8[2] as f32 / 255.;
    let (pitch_limits, head_limits) = match mode {
        1 => ([-15.5, 25.], [-35., 20.]),
        2 => ([-25.5 - crouch * 10., 10.], [crouch * 10. - 35., 35.]),
        3 => ([-25.5, 25.], [-35., 20.]),
        5 => ([-15.5, 15.], [-35., 20.]),
        _ => ([-12.5, 10.], [-45., 35.]),
    };
    // The yaw is applied in model space. Rebuild children after each rotation
    // before converting the four changed transforms back to parent space.
    if yaw.abs() > 0.001 {
        let angle = remap(yaw, [-55., 65.]);
        for (i, weight) in [0.1, 0.2, 0.3, 0.4].into_iter().enumerate() {
            model[i + 1].rotation = multiply(z_rotation(angle * weight), model[i + 1].rotation);
            for j in i + 2..5 {
                model[j] = Transform::compose(model[j - 1], local[j - 1]);
            }
        }
        for i in 0..4 {
            local[i] = Transform::compose(model[i].inverse()?, model[i + 1]);
        }
    }
    // Pitch is applied in parent space, using a separate neck/head range.
    if pitch.abs() > 0.001 {
        let angle = remap(pitch, pitch_limits);
        for (bone, weight) in local[..4].iter_mut().zip([0.125, 0.225, 0.25, 0.4]) {
            bone.rotation = multiply(bone.rotation, z_rotation(angle * weight));
        }
        let angle = remap(pitch, head_limits);
        for (bone, weight) in local[4..].iter_mut().zip([0.35, 0.65]) {
            bone.rotation = multiply(bone.rotation, z_rotation(angle * weight));
        }
    }
    // Commit only after all prerequisites and inverses succeeded.
    for (&index, transform) in chain[1..].iter().zip(local) {
        pose.local[index] = Some(transform);
    }
    Ok(mode != 3)
}

/// Evaluates torso, weapon, mode-3 head correction and enabled arm IK. Unknown
/// inputs remain absent; the caller rebuilds model transforms from these locals.
pub fn apply(
    skeleton: &Skeleton,
    pose: &mut Pose,
    normalized16: [u16; 2],
    normalized8: [u8; 4],
    mode: u8,
    subtype: u8,
) -> Result<bool> {
    ensure!(
        pose.local.len() == skeleton.names.len() && pose.model.len() == skeleton.names.len(),
        "Incomplete AimCS pose"
    );
    let index = |name: &str| {
        skeleton
            .names
            .iter()
            .position(|n| n == name)
            .context("Missing AimCS bone")
    };
    let pivot_index = index("wpnPivot")?;
    let weapon_index = index("wpn")?;
    let tip_index = index("wpnTip")?;
    let end_index = index("wpnEnd")?;
    let head_index = index("head_0")?;
    ensure!(
        skeleton.parents[pivot_index] == Some(index("root_motion")?)
            && skeleton.parents[weapon_index] == Some(pivot_index)
            && skeleton.parents[tip_index] == Some(weapon_index)
            && skeleton.parents[end_index] == Some(weapon_index),
        "Unsupported AimCS weapon hierarchy"
    );
    let original_pivot = pose.local[pivot_index].context("Unknown AimCS weapon pivot")?;
    let mut weapon_local = pose.local[weapon_index].context("Unknown AimCS weapon")?;
    let tip_local = pose.local[tip_index].context("Unknown AimCS weapon tip")?;
    let end_local = pose.local[end_index].context("Unknown AimCS weapon end")?;
    let restriction = normalized8[1] as f32 / 255.;
    let crouch = normalized8[2] as f32 / 255.;
    let yaw = normalized16[0] as f32 / 65535. * 180. - 90.;
    let pitch = normalized16[1] as f32 / 65535. * 180. - 90.;
    let pitch = pitch * (90. - restriction * 40.) / 90.;
    let shift = normalized8[3] as f32 / 255. * 20. - 10.;
    let shift = (1. - (pitch.abs() / 70.).clamp(0., 1.)) * (1. - restriction) * shift;
    if shift.abs() > 0.001 {
        let mut weapon = Transform::compose(original_pivot, weapon_local);
        weapon.position[0] -= shift.abs() * 0.75;
        weapon.position[2] -= shift;
        weapon_local = Transform::compose(original_pivot.inverse()?, weapon);
    }
    apply_spine(skeleton, pose, normalized16, normalized8, mode)?;
    let mut parent = pose.model[index("pelvis")?].context("Unknown AimCS pelvis")?;
    let mut torso = [Transform::IDENTITY; 6];
    for (i, name) in [
        "spine_0", "spine_1", "spine_2", "spine_3", "neck_0", "head_0",
    ]
    .iter()
    .enumerate()
    {
        parent = Transform::compose(
            parent,
            pose.local[index(name)?].context("Unknown AimCS torso")?,
        );
        torso[i] = parent;
    }
    let mut pivot = torso[3];
    let mut weapon = Transform::compose(pivot, weapon_local);
    if mode == 3 {
        let mut end = Transform::compose(weapon, end_local);
        let relative = Transform::compose(end.inverse()?, weapon);
        let angle = remap(pitch, [-75. - 15. * crouch, 80.]) * (1. - 0.5 * restriction);
        end.rotation = multiply(end.rotation, y_rotation(angle));
        weapon = Transform::compose(end, relative);
    } else if matches!(mode, 1 | 2 | 5) {
        let limits = match mode {
            1 => [-35., 25.],
            2 => [-40., 55.],
            _ => [-15., 5.],
        };
        let angle = remap(pitch, limits)
            * if mode == 2 {
                1. - 0.5 * restriction
            } else {
                1.
            };
        pivot.rotation = multiply(y_rotation(angle), pivot.rotation);
        let fraction = yaw.abs() / 90. * pitch.abs() / 90.;
        let roll = fraction * if pitch < 0. { 5. } else { -15. };
        if roll.abs() > 0.001 {
            weapon = Transform::compose(pivot, weapon_local);
            let delta = std::array::from_fn::<_, 3, _>(|i| weapon.position[i] - pivot.position[i]);
            let axis = unit([delta[1], -delta[0], 0.])?;
            pivot.rotation = multiply(axis_rotation(axis, roll), pivot.rotation);
        }
        weapon = Transform::compose(pivot, weapon_local);
        if mode == 2 {
            let turn = fraction * if yaw < 0. { -45. } else { 45. };
            if turn.abs() > 0.001 {
                weapon.rotation = multiply(z_rotation(turn), weapon.rotation);
            }
        }
    }

    // Native weapon-type offsets are applied only outside the neutral pitch range.
    let mut low = -30.;
    let mut high = 50.;
    let mut low_offset = [-10., 3., -5.];
    let mut high_offset = [-1. - 7. * crouch, 3., 4. + 8. * crouch];
    match subtype {
        2 => high_offset = [2., 0., 4.],
        3 => {
            low_offset = [0.; 3];
            high_offset = [3., 2., 4.];
        }
        4 => high_offset = [-6., 2., 6.],
        5 => high_offset = [6., 2., 4.],
        6 => {
            low_offset = [-2., 0., 0.];
            high_offset = [0.; 3];
        }
        7 => high_offset = [-5., 1., 3.],
        8 => {
            low_offset = [-6., 4., -16.];
            high_offset = [-8., 0., 14.];
            high = 20.;
        }
        _ => {}
    }
    if mode == 2 {
        low = -70.;
        low_offset = [0.; 3];
        high_offset = if subtype == 1 {
            [-2., 0., 4.]
        } else {
            [0., 0., 1.]
        };
    }
    if !matches!(mode, 2 | 3) {
        low_offset = [0.; 3];
        high_offset = [0.; 3];
    }
    let offset = if pitch <= low {
        low_offset.map(|v| v * (1. - ((pitch + 90.) / (low + 90.)).clamp(0., 1.)))
    } else if pitch >= high {
        high_offset.map(|v| v * ((pitch - high) / (90. - high)).clamp(0., 1.))
    } else {
        [0.; 3]
    };
    let mut relative = Transform::compose(pivot.inverse()?, weapon);
    let offset = rotate(relative.rotation, offset);
    relative.position = std::array::from_fn(|i| relative.position[i] + offset[i]);
    weapon = Transform::compose(pivot, relative);
    if matches!(mode, 2 | 3) && restriction < 1. {
        let tip = Transform::compose(weapon, tip_local);
        let mut end = Transform::compose(weapon, end_local);
        let relative = Transform::compose(end.inverse()?, weapon);
        let direction = std::array::from_fn(|i| tip.position[i] - end.position[i]);
        let (sp, cp) = pitch.to_radians().sin_cos();
        let (sy, cy) = remap(yaw, [-55., 65.]).to_radians().sin_cos();
        let correction = between(direction, [cp * cy, cp * sy, -sp])?;
        let correction =
            super::animation_pose::fast_slerp(correction, [0., 0., 0., 1.], restriction)?;
        end.rotation = multiply(correction, end.rotation);
        weapon = Transform::compose(end, relative);
    }
    if mode == 3 && pitch > -25. {
        let tip = Transform::compose(weapon, tip_local);
        let mut head = torso[5];
        let neck = torso[4];
        let forward = rotate(head.rotation, [0., 1., 0.]);
        let direction = std::array::from_fn(|i| tip.position[i] - head.position[i]);
        let target = multiply(between(forward, direction)?, head.rotation);
        head.rotation = super::animation_pose::fast_slerp(
            head.rotation,
            target,
            ((pitch + 25.) / 115.).clamp(0., 1.),
        )?;
        pose.local[head_index] = Some(Transform::compose(neck.inverse()?, head));
    }
    if matches!(mode, 2 | 3) {
        pivot.rotation = multiply(pivot.rotation, original_pivot.inverse()?.rotation);
        pose.local[weapon_index] = Some(Transform::compose(pivot.inverse()?, weapon));
    } else {
        pose.local[weapon_index] = Some(weapon_local);
    }
    pose.local[pivot_index] = Some(pivot);
    let left_enabled = matches!(mode, 0 | 2 | 3) || (mode == 5 && subtype == 9);
    for (enabled, hand, attachment) in [
        (left_enabled, "hand_L", "wpnHand_L"),
        (true, "hand_R", "wpnHand_R"),
    ] {
        if enabled {
            let target = pose.local[index(attachment)?].map(|t| Transform::compose(weapon, t));
            skeleton.solve_foot(
                &mut pose.local,
                index(hand)?,
                target,
                false,
                normalized8[0] as f32 / 255.,
            )?;
        }
    }
    Ok(true)
}

/// Snaps the primary weapon to its recorded right-hand attachment. Only flag 1
/// additionally solves the primary left arm; flags 2/3 modify secondary weapon
/// skeletons afterward and have the same primary output as flag 0.
pub fn apply_snap(skeleton: &Skeleton, pose: &mut Pose, flags: u8) -> Result<()> {
    ensure!(
        flags <= 3 && !pose.is_additive,
        "Unsupported SnapWeapon task"
    );
    let index = |name: &str| {
        skeleton
            .names
            .iter()
            .position(|n| n == name)
            .context("Missing SnapWeapon bone")
    };
    let weapon = index("wpn")?;
    let right_attachment = index("wpnHand_R")?;
    let left_attachment = index("wpnHand_L")?;
    let right_hand = index("hand_R")?;
    let left_hand = index("hand_L")?;
    ensure!(
        skeleton.parents[right_attachment] == Some(weapon)
            && skeleton.parents[left_attachment] == Some(weapon),
        "Unsupported SnapWeapon hierarchy"
    );
    let parent = skeleton.parents[weapon].context("Missing SnapWeapon parent")?;
    let target = match (
        skeleton.model_bone(&pose.local, right_hand)?,
        pose.local[right_attachment],
    ) {
        (Some(hand), Some(attachment)) => Some(Transform::compose(hand, attachment.inverse()?)),
        _ => None,
    };
    pose.local[weapon] = match (skeleton.model_bone(&pose.local, parent)?, target) {
        (Some(parent), Some(target)) => Some(Transform::compose(parent.inverse()?, target)),
        _ => None,
    };
    if flags == 1 {
        let target = target
            .zip(pose.local[left_attachment])
            .map(|(weapon, attachment)| Transform::compose(weapon, attachment));
        skeleton.solve_foot(&mut pose.local, left_hand, target, false, 1.)?;
    }
    Ok(())
}

#[cfg(test)]
fn model_pose(skeleton: &Skeleton, pose: &Pose) -> Result<Vec<Option<Transform>>> {
    ensure!(
        pose.local.len() == skeleton.parents.len(),
        "Incomplete AimCS pose"
    );
    let mut model: Vec<Option<Transform>> = Vec::with_capacity(pose.local.len());
    for (i, local) in pose.local.iter().enumerate() {
        let transform = match (skeleton.parents[i], local) {
            (_, None) => None,
            (None, Some(local)) => Some(*local),
            (Some(parent), Some(local)) => {
                ensure!(parent < i, "Invalid AimCS parent order");
                model[parent].map(|parent| Transform::compose(parent, *local))
            }
        };
        model.push(transform);
    }
    Ok(model)
}

fn rotate(rotation: [f32; 4], position: [f32; 3]) -> [f32; 3] {
    Transform::compose(
        Transform {
            rotation,
            ..Transform::IDENTITY
        },
        Transform {
            position,
            ..Transform::IDENTITY
        },
    )
    .position
}

fn y_rotation(degrees: f32) -> [f32; 4] {
    let (sin, cos) = (degrees.to_radians() * 0.5).sin_cos();
    [0., sin, 0., cos]
}

fn axis_rotation(axis: [f32; 3], degrees: f32) -> [f32; 4] {
    let (sin, cos) = (degrees.to_radians() * 0.5).sin_cos();
    [axis[0] * sin, axis[1] * sin, axis[2] * sin, cos]
}

fn unit(v: [f32; 3]) -> Result<[f32; 3]> {
    let length = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    ensure!(
        length.is_finite() && length > 0.000001,
        "Degenerate AimCS axis"
    );
    Ok(v.map(|x| x / length))
}

fn between(a: [f32; 3], b: [f32; 3]) -> Result<[f32; 4]> {
    let a = unit(a)?;
    let b = unit(b)?;
    // The native shortest-arc constructor uses the half vector, including its
    // antiparallel fallback; it is not the cross(a,b), 1+dot(a,b) rearrangement.
    let half = std::array::from_fn::<_, 3, _>(|i| (a[i] + b[i]) * 0.5);
    let q = if half.iter().map(|v| v * v).sum::<f32>() > f32::EPSILON {
        [
            a[1] * half[2] - a[2] * half[1],
            a[2] * half[0] - a[0] * half[2],
            a[0] * half[1] - a[1] * half[0],
            a.iter().zip(half).map(|(x, y)| x * y).sum(),
        ]
    } else if a[0].abs() > 0.5 {
        [a[1], -a[0], 0., 0.]
    } else {
        [0., a[2], -a[1], 0.]
    };
    let length = q.iter().map(|x| x * x).sum::<f32>().sqrt();
    ensure!(
        length.is_finite() && length > 0.000001,
        "Opposite AimCS directions"
    );
    Ok(q.map(|v| v / length))
}

fn remap(angle: f32, limits: [f32; 2]) -> f32 {
    if angle < 0. {
        let t = ((angle + 90.) / 90.).clamp(0., 1.);
        limits[0] + t * -limits[0]
    } else {
        (angle / 90.).clamp(0., 1.) * limits[1]
    }
}

fn z_rotation(degrees: f32) -> [f32; 4] {
    let (sin, cos) = (degrees.to_radians() * 0.5).sin_cos();
    [0., 0., sin, cos]
}

fn multiply(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [x, y, z, w] = a;
    let [xx, yy, zz, ww] = b;
    [
        w * xx + x * ww + y * zz - z * yy,
        w * yy - x * zz + y * ww + z * xx,
        w * zz + x * yy - y * xx + z * ww,
        w * ww - x * xx - y * yy - z * zz,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Deserialize)]
    struct Fixture {
        local: Vec<Transform>,
        world: Transform,
        points: Vec<Point>,
    }
    #[derive(Deserialize)]
    struct Point {
        bone: usize,
        offset: [f32; 3],
        expected: [f32; 3],
    }

    #[test]
    fn reconstructed_spine_matches_independent_game_attachments() -> Result<()> {
        // Actual decoded clips plus captured head/spine attachments from one idle pose.
        // No player identifiers or demo file names are retained in this fixture.
        let fixture: Fixture = serde_json::from_str(
            r#"{"local":[{"position":[0.0,0.0,0.0],"rotation":[0.0,0.0,0.0,1.0],"scale":1.0},{"position":[-3.726933002471924,5.380821228027344,38.64374542236328],"rotation":[-0.6063159704208374,-0.3389909863471985,-0.6534370183944702,0.30080899596214294],"scale":1.0},{"position":[0.9342039823532104,0.09050799906253815,0.0],"rotation":[-0.04848995432257652,-0.013349278829991817,-0.10999184846878052,-0.9926592111587524],"scale":1.0},{"position":[4.066376209259033,0.0,0.0],"rotation":[-0.022182932123541832,0.001787024550139904,-0.06349469721317291,-0.9977340698242188],"scale":1.0},{"position":[4.744082927703857,0.0,0.0],"rotation":[-0.0006043808534741402,0.0085170678794384,0.0807696282863617,-0.9966962337493896],"scale":1.0},{"position":[6.347774982452393,0.0,0.0],"rotation":[-0.02441547065973282,-0.009546286426484585,-0.21360930800437927,-0.9765674471855164],"scale":1.0},{"position":[6.515209197998047,0.0,0.0],"rotation":[-0.003966525197029114,0.0432574562728405,-0.14662136137485504,-0.9882384538650513],"scale":1.0},{"position":[5.76214599609375,0.0,0.0],"rotation":[-0.1453203707933426,0.08435274660587311,0.3121044337749481,-0.935070812702179],"scale":1.0}],"world":{"position":[258.1593933105469,2480.5537109375,-121.91265106201172],"rotation":[0,0,-0.38377658364502987,0.9234259763758811],"scale":1},"points":[{"bone":7,"offset":[7.0,6.0,0.0],"expected":[266.09503173828125,2472.3330078125,-51.70854949951172]},{"bone":2,"offset":[-0.935169,-3.114181,-7.750176],"expected":[251.0554962158203,2488.163330078125,-83.33253479003906]},{"bone":5,"offset":[-2.364758,-9.845432,0.65404],"expected":[256.10028076171875,2493.692626953125,-64.54926300048828]}]}"#,
        )?;
        let skeleton = Skeleton::from_value(serde_json::json!({
            "m_ID":"animation/skeletons/characters/worldmodel.vnmskel",
            "m_boneIDs":["root_motion","pelvis","spine_0","spine_1","spine_2","spine_3","neck_0","head_0"],
            "m_parentIndices":[-1,0,1,2,3,4,5,6],
            "m_parentSpaceReferencePose":fixture.local.iter().map(|t| {
                [t.position[0],t.position[1],t.position[2],t.scale,t.rotation[0],t.rotation[1],t.rotation[2],t.rotation[3]]
            }).collect::<Vec<_>>()
        }))?;
        let mut pose = Pose {
            local: fixture.local.iter().copied().map(Some).collect(),
            model: Vec::new(),
            is_additive: false,
        };
        for (i, &local) in fixture.local.iter().enumerate() {
            pose.model.push(Some(if i == 0 {
                local
            } else {
                Transform::compose(pose.model[i - 1].expect("parent"), local)
            }));
        }
        assert!(apply_spine(
            &skeleton,
            &mut pose,
            [33283, 31736],
            [255, 0, 0, 128],
            1
        )?);
        let mut world = Vec::new();
        for (i, local) in pose.local.iter().enumerate() {
            world.push(Transform::compose(
                if i == 0 { fixture.world } else { world[i - 1] },
                local.expect("measured"),
            ));
        }
        for point in &fixture.points {
            let position = Transform::compose(
                world[point.bone],
                Transform {
                    position: point.offset,
                    ..Transform::IDENTITY
                },
            )
            .position;
            let error = position
                .iter()
                .zip(point.expected)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                .sqrt();
            assert!(error < 0.001, "bone {} error {error}", point.bone);
        }
        assert!(!apply_spine(
            &skeleton,
            &mut pose,
            [32768, 32768],
            [0; 4],
            3
        )?);
        let before = pose.local.clone();
        assert!(apply_spine(&skeleton, &mut pose, [0; 2], [0; 4], 7).is_err());
        assert_eq!(pose.local, before);
        Ok(())
    }
    #[derive(Deserialize)]
    struct Mode3Fixture {
        local: Vec<Transform>,
        names: Vec<String>,
        parents: Vec<Option<usize>>,
        world: Transform,
        args: AimArgs,
        expected: [f32; 3],
        #[serde(default)]
        hands: Option<[[f32; 3]; 2]>,
    }
    #[derive(Deserialize)]
    struct AimArgs {
        normalized16: [u16; 2],
        normalized8: [u8; 4],
        mode: u8,
        flags5: u8,
        #[serde(default)]
        snap: u8,
    }

    #[test]
    fn head_and_hands_match_independent_game_captures() -> Result<()> {
        let opposite = between([1., 0., 0.], [-1., 0., 0.])?;
        assert_eq!(rotate(opposite, [1., 0., 0.]), [-1., 0., 0.]);
        let fixtures: Vec<Mode3Fixture> =
            serde_json::from_str(include_str!("animation_aim_cases.json"))?;
        assert_eq!(fixtures.iter().filter(|f| f.hands.is_none()).count(), 7);
        for fixture in fixtures {
            let skeleton = Skeleton::from_value(serde_json::json!({
                "m_ID":"animation/skeletons/characters/worldmodel.vnmskel",
                "m_boneIDs":fixture.names,
                "m_parentIndices":fixture.parents.iter().map(|p|p.map_or(-1,|p|p as i32)).collect::<Vec<_>>(),
                "m_parentSpaceReferencePose":fixture.local.iter().map(|t| {
                    [t.position[0],t.position[1],t.position[2],t.scale,t.rotation[0],t.rotation[1],t.rotation[2],t.rotation[3]]
                }).collect::<Vec<_>>()
            }))?;
            let mut pose = Pose {
                local: fixture.local.into_iter().map(Some).collect(),
                model: Vec::new(),
                is_additive: false,
            };
            pose.model = model_pose(&skeleton, &pose)?;
            assert!(apply(
                &skeleton,
                &mut pose,
                fixture.args.normalized16,
                fixture.args.normalized8,
                fixture.args.mode,
                fixture.args.flags5
            )?);
            let before_snap = pose.clone();
            apply_snap(&skeleton, &mut pose, fixture.args.snap)?;
            let mut mode0 = before_snap.clone();
            apply_snap(&skeleton, &mut mode0, 0)?;
            for flags in [2, 3] {
                let mut secondary = before_snap.clone();
                apply_snap(&skeleton, &mut secondary, flags)?;
                assert_eq!(
                    secondary.local, mode0.local,
                    "secondary Snap changed primary pose"
                );
            }
            let model = model_pose(&skeleton, &pose)?;
            let head = Transform::compose(fixture.world, model[7].context("missing head")?);
            // clip_limit is the native model attachment on head_0.
            let point = Transform::compose(
                head,
                Transform {
                    position: [7.0, 6.0, 0.0],
                    ..Transform::IDENTITY
                },
            )
            .position;
            let error = point
                .iter()
                .zip(fixture.expected)
                .map(|(a, b)| (a - b) * (a - b))
                .sum::<f32>()
                .sqrt();
            assert!(
                error < 0.001,
                "mode {} subtype {} head error {error}",
                fixture.args.mode,
                fixture.args.flags5
            );
            if let Some(hands) = fixture.hands {
                for (i, expected) in hands.into_iter().enumerate() {
                    let transform = Transform::compose(
                        fixture.world,
                        model[if i == 0 { 11 } else { 15 }].context("missing hand")?,
                    );
                    let offset = if i == 0 {
                        [2.6, 1.4, 0.0]
                    } else {
                        [-2.6, -1.4, 0.0]
                    };
                    let actual = Transform::compose(
                        transform,
                        Transform {
                            position: offset,
                            ..Transform::IDENTITY
                        },
                    )
                    .position;
                    let error = actual
                        .iter()
                        .zip(expected)
                        .map(|(a, b)| (a - b) * (a - b))
                        .sum::<f32>()
                        .sqrt();
                    // Includes quantized task parameters and two successive IK solves;
                    // the captured world-space reference is not bit-identical to decoded input.
                    assert!(
                        error < 0.01,
                        "mode {} subtype {} hand {} error {error}",
                        fixture.args.mode,
                        fixture.args.flags5,
                        i
                    );
                }
            }
        }
        Ok(())
    }
}
