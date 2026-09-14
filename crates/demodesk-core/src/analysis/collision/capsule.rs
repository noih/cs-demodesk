//! Generic CRnCapsuleShape line contacts, translated from the pinned vphysics2
//! 157970/15A4E0 nearest query and 2353A0 forward/reverse collector.
//! Player hitbox selection uses A032C0/A03F90; pose/time qualification is external.
use super::Contact;
use anyhow::{ensure, Result};

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn dot_zyx(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[2] * b[2] + a[1] * b[1]) + a[0] * b[0]
}
fn square_xyz(v: [f32; 3]) -> f32 {
    (v[0] * v[0] + v[1] * v[1]) + v[2] * v[2]
}
fn normalize(v: [f32; 3]) -> [f32; 3] {
    // 83A70 has a double-precision fallback outside this native length range.
    let length = ((v[1] * v[1] + v[2] * v[2]) + v[0] * v[0]).sqrt();
    if length == 0. {
        return [0.; 3];
    }
    if (1e-17..=1e17).contains(&length) {
        let inv = 1. / length;
        return v.map(|x| x * inv);
    }
    let d = v.map(f64::from);
    let inv = 1. / ((d[1] * d[1] + d[0] * d[0]) + d[2] * d[2]).sqrt();
    d.map(|x| (x * inv) as f32)
}
fn orthogonal(axis: [f32; 3]) -> [f32; 3] {
    // 83C00 constructs, projects, then normalizes this particular candidate.
    let candidate = normalize([
        (1. - axis[2]) * (axis[1] * axis[1] - 0.) + axis[2],
        0.,
        -axis[0],
    ]);
    let along = dot_zyx(axis, candidate);
    normalize(std::array::from_fn(|i| candidate[i] - along * axis[i]))
}
fn sphere(relative: [f32; 3], delta: [f32; 3], radius: f32) -> Option<(Contact, bool)> {
    let speed2 = dot_zyx(delta, delta);
    let length2 =
        (relative[1] * relative[1] + relative[2] * relative[2]) + relative[0] * relative[0];
    let c = length2 - radius * radius;
    if speed2 >= f32::from_bits(0x28800000) {
        let b = dot_zyx(relative, delta);
        let discriminant = b * b - c * speed2;
        if discriminant < 0. {
            return None;
        }
        let t = (-b - discriminant.sqrt()) / speed2;
        if t >= 0. {
            return (t < 1.).then(|| {
                (
                    Contact {
                        fraction: t,
                        normal: std::array::from_fn(|i| {
                            (t * delta[i] + relative[i]) * (1. / radius)
                        }),
                        exit: false,
                    },
                    false,
                )
            });
        }
    }
    if c > 0. {
        return None;
    }
    let normal = if length2 > f32::EPSILON {
        let inv = 1. / length2.sqrt();
        relative.map(|x| x * inv)
    } else {
        [0., 0., 1.]
    };
    Some((
        Contact {
            fraction: 0.,
            normal,
            exit: false,
        },
        true,
    ))
}
fn nearest_with_solid(
    a: [f32; 3],
    b: [f32; 3],
    radius: f32,
    start: [f32; 3],
    delta: [f32; 3],
) -> Option<(Contact, bool)> {
    if radius < 1e-5 {
        return None;
    }
    let raw = sub(b, a);
    let length = ((raw[1] * raw[1] + raw[0] * raw[0]) + raw[2] * raw[2]).sqrt();
    let relative = sub(start, a);
    if length < 0.001 {
        return sphere(relative, delta, radius);
    }
    let inv = 1. / length;
    let axis = raw.map(|x| x * inv);
    let ray_length = square_xyz(delta).sqrt();
    if ray_length <= 0.0001 {
        let along = dot_zyx(axis, relative);
        let off = if along < 0. {
            relative
        } else if along > length {
            sub(start, b)
        } else {
            std::array::from_fn(|i| relative[i] - axis[i] * along)
        };
        let squared = square_xyz(off);
        if squared > radius * radius {
            return None;
        }
        let normal = if squared > 1e-8 {
            let inv = 1. / squared.sqrt();
            off.map(|x| x * inv)
        } else {
            orthogonal(axis)
        };
        return Some((
            Contact {
                fraction: 0.,
                normal,
                exit: false,
            },
            true,
        ));
    }
    let inverse_ray = 1. / ray_length;
    let plane = cross(axis, delta.map(|x| x * inverse_ray));
    let plane_squared = (plane[1] * plane[1] + plane[0] * plane[0]) + plane[2] * plane[2];
    let along_delta = dot_zyx(axis, delta);
    let along_start = dot_zyx(axis, relative);
    let mut fraction = f32::MAX;
    let mut closest = 0.;
    if plane_squared > f32::from_bits(0x30800000) {
        let inv = 1. / plane_squared.sqrt();
        let plane = plane.map(|x| x * inv);
        let offset = dot_zyx(plane, relative);
        let effective = radius * radius - offset * offset;
        if effective <= 0. {
            return None;
        }
        let side = cross(plane, axis);
        let side_start = dot_zyx(side, relative);
        let side_delta = dot_zyx(side, delta);
        closest = along_start.max(0.).min(length);
        let distance = along_start - closest;
        let side_squared = side_start * side_start;
        if effective > distance * distance + side_squared {
            fraction = 0.;
        } else {
            let inv = 1. / (side_delta * side_delta + along_delta * along_delta);
            let side_product = side_delta * side_start;
            let candidate = (-effective.sqrt() - side_start) / side_delta;
            closest = 0_f32.max(candidate) * along_delta + along_start;
            let endcap = if closest < 0. {
                closest = 0.;
                Some(along_start)
            } else if closest < length {
                if candidate >= 0. {
                    fraction = candidate;
                }
                None
            } else {
                closest = length;
                Some(along_start - length)
            };
            if let Some(along) = endcap {
                let bb = (along * along_delta + side_product) * inv;
                let cc = ((along * along + side_squared) - effective) * inv;
                let discriminant = bb * bb - cc;
                if discriminant >= 0. {
                    let candidate = -bb - discriminant.sqrt();
                    if candidate >= 0. {
                        fraction = candidate;
                    }
                }
            }
        }
    } else {
        let off: [f32; 3] = std::array::from_fn(|i| relative[i] - axis[i] * along_start);
        let squared = square_xyz(off);
        let reach = if squared < f32::from_bits(0x28800000) {
            Some(radius)
        } else {
            let inv = 1. / squared.sqrt();
            let perpendicular = cross(off.map(|x| x * inv), axis);
            let residual = dot_zyx(perpendicular, relative);
            let effective = (radius * radius - squared) - residual * residual;
            (effective > 0.).then(|| effective.sqrt())
        };
        if let Some(reach) = reach {
            if along_start + reach < 0. {
                closest = 0.;
                let distance = -(along_start + reach);
                if along_delta >= distance {
                    fraction = distance / along_delta;
                }
            } else if length + reach <= along_start {
                closest = length;
                let distance = along_start - (length + reach);
                if -along_delta >= distance {
                    fraction = distance / (-along_delta);
                }
            } else {
                closest = along_start.max(0.).min(length);
                fraction = 0.;
            }
        }
    }
    let fraction = 1_f32.min(fraction);
    if fraction >= 1. {
        return None;
    }
    let point: [f32; 3] = std::array::from_fn(|i| fraction * delta[i] + start[i]);
    let normal = normalize(std::array::from_fn(|i| {
        point[i] - (closest * axis[i] + a[i])
    }));
    Some((
        Contact {
            fraction,
            normal,
            exit: false,
        },
        fraction <= 0.,
    ))
}

fn nearest(
    a: [f32; 3],
    b: [f32; 3],
    radius: f32,
    start: [f32; 3],
    delta: [f32; 3],
) -> Option<Contact> {
    nearest_with_solid(a, b, radius, start, delta).map(|(contact, _)| contact)
}

/// Contacts for one generic capsule, in its coordinate space. Collection across
/// shapes, material identity, transforms and player rewinding remain caller work.
/// An exit is found by the native reverse query, not by unioning body capsules.
pub fn contacts(
    a: [f32; 3],
    b: [f32; 3],
    radius: f32,
    start: [f32; 3],
    delta: [f32; 3],
    max_fraction: f32,
) -> Result<Vec<Contact>> {
    ensure!(
        a.iter()
            .chain(&b)
            .chain(&start)
            .chain(&delta)
            .all(|v| v.is_finite())
            && radius.is_finite()
            && radius >= 0.
            && max_fraction.is_finite(),
        "invalid generic capsule query"
    );
    let Some(entry) = nearest(a, b, radius, start, delta) else {
        return Ok(Vec::new());
    };
    if entry.fraction > max_fraction {
        return Ok(Vec::new());
    }
    let end = std::array::from_fn(|i| delta[i] + start[i]);
    let Some(mut exit) = nearest(a, b, radius, end, delta.map(|x| -x)) else {
        return Ok(Vec::new());
    };
    exit.fraction = 1. - exit.fraction;
    exit.exit = true;
    if exit.fraction <= entry.fraction {
        return Ok(Vec::new());
    }
    Ok(vec![entry, exit])
}

/// One caller-qualified hitbox in native source iteration order. The surface
/// name is borrowed from immutable model metadata; no pose is selected here.
#[derive(Clone, Copy, Debug)]
pub struct HitboxCapsule<'a> {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub radius: f32,
    pub group: u32,
    /// Serialized m_nHitBoxIndex metadata, not the native trace remapped bone index.
    pub index: u32,
    pub surface: &'a str,
}
#[derive(Clone, Copy, Debug)]
pub struct SelectedHitbox<'a> {
    pub source_ordinal: usize,
    /// Serialized m_nHitBoxIndex metadata, not the native trace remapped bone index.
    pub index: u32,
    pub group: u32,
    pub surface: &'a str,
    pub entry: Contact,
    pub exit: Contact,
}
fn bucket(group: u32) -> usize {
    match group {
        1 => 0,
        8 => 1,
        2 => 2,
        3 => 3,
        4 | 5 => 4,
        6 | 7 => 5,
        _ => 6,
    }
}
/// A032C0 group selection followed by A03F90 reverse trace of the selected shape.
/// Callers must supply native-order, temporally qualified world geometry.
/// This does not union intersected hitboxes or infer a rewind from packet poses.
pub fn player_contacts<'a>(
    hitboxes: &[HitboxCapsule<'a>],
    start: [f32; 3],
    delta: [f32; 3],
    max_fraction: f32,
) -> Result<Option<SelectedHitbox<'a>>> {
    ensure!(
        start.iter().chain(&delta).all(|x| x.is_finite()) && max_fraction.is_finite(),
        "invalid player capsule ray"
    );
    let mut groups: [Option<(usize, Contact)>; 7] = [None; 7];
    for (ordinal, h) in hitboxes.iter().enumerate() {
        ensure!(
            h.a.iter().chain(&h.b).all(|x| x.is_finite())
                && h.radius.is_finite()
                && h.radius >= 0.
                && !h.surface.is_empty(),
            "invalid qualified hitbox capsule"
        );
        let Some((mut contact, start_solid)) = nearest_with_solid(h.a, h.b, h.radius, start, delta)
        else {
            continue;
        };
        if contact.fraction >= max_fraction {
            continue;
        }
        let slot = &mut groups[bucket(h.group)];
        if slot
            .as_ref()
            .is_some_and(|(_, old)| contact.fraction > old.fraction)
        {
            continue;
        }
        // Native ties replace the earlier source descriptor. A03B2F also uses
        // this fixed normal only for a player trace marked start-solid.
        if start_solid {
            contact.normal = [1., 0., 0.];
        }
        *slot = Some((ordinal, contact));
    }
    if let Some((_, head)) = groups[0] {
        if groups[1..3]
            .iter()
            .flatten()
            .any(|(_, other)| head.fraction > other.fraction)
        {
            groups[0] = None;
        }
    }
    let Some((ordinal, entry)) = groups.into_iter().flatten().next() else {
        return Ok(None);
    };
    let h = &hitboxes[ordinal];
    let end = std::array::from_fn(|i| delta[i] + start[i]);
    // A03F90 keeps the initialized exit=1 and zero normal on reverse miss;
    // unlike generic contacts(), it does not require a successful reverse hit.
    let exit = match nearest(h.a, h.b, h.radius, end, delta.map(|x| -x)) {
        Some(mut hit) => {
            hit.fraction = 1. - hit.fraction;
            hit.exit = true;
            hit
        }
        None => Contact {
            fraction: 1.,
            normal: [0.; 3],
            exit: true,
        },
    };
    Ok(Some(SelectedHitbox {
        source_ordinal: ordinal,
        index: h.index,
        group: h.group,
        surface: h.surface,
        entry,
        exit,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn player_group_priority_ties_and_selected_shape_exit() {
        let shape = |x, group, index| HitboxCapsule {
            a: [x, 0., -1.],
            b: [x, 0., 1.],
            radius: 1.,
            group,
            index,
            surface: "playerflesh",
        };
        let start = [0.; 3];
        let delta = [20., 0., 0.];
        // Chest in front suppresses head; closer arms do not supersede chest.
        let input = [shape(8., 1, 0), shape(6., 2, 1), shape(3., 4, 2)];
        let hit = player_contacts(&input, start, delta, 1.).unwrap().unwrap();
        assert_eq!((hit.index, hit.source_ordinal), (1, 1));
        assert_eq!(hit.entry.fraction, 0.25);
        assert_eq!(hit.exit.fraction, 1. - 0.65_f32);
        // Arms do not suppress head. Equal-distance head/chest retains head.
        let input = [shape(8., 1, 0), shape(8., 2, 1), shape(3., 4, 2)];
        assert_eq!(
            player_contacts(&input, start, delta, 1.)
                .unwrap()
                .unwrap()
                .index,
            0
        );
        // Input order, rather than serialized index, breaks same-bucket ties.
        let input = [shape(5., 4, 99), shape(5., 5, 2)];
        assert_eq!(
            player_contacts(&input, start, delta, 1.)
                .unwrap()
                .unwrap()
                .index,
            2
        );
        let input = [shape(0., 3, 3)];
        assert_eq!(
            player_contacts(&input, start, delta, 1.)
                .unwrap()
                .unwrap()
                .entry
                .normal,
            [1., 0., 0.]
        );
        assert!(player_contacts(&[], start, delta, 1.).unwrap().is_none());
        assert!(player_contacts(&[shape(6., 2, 1)], start, delta, 0.25)
            .unwrap()
            .is_none());
    }
    #[test]
    fn generic_capsule_side_caps_sphere_and_zero_segment() {
        let a = [0., 0., -2.];
        let b = [0., 0., 2.];
        let side = contacts(a, b, 1., [-2., 0., 0.], [4., 0., 0.], 1.).unwrap();
        assert_eq!(
            side.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            [0.25, 0.75]
        );
        assert_eq!(side[0].normal, [-1., 0., 0.]);
        assert_eq!(side[1].normal, [1., 0., 0.]);
        let axial = contacts(a, b, 1., [0., 0., -4.], [0., 0., 8.], 1.).unwrap();
        assert_eq!(
            axial.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            [0.125, 0.875]
        );
        let sphere = contacts([0.; 3], [0.; 3], 1., [-2., 0., 0.], [4., 0., 0.], 1.).unwrap();
        assert_eq!(
            sphere.iter().map(|c| c.fraction).collect::<Vec<_>>(),
            [0.25, 0.75]
        );
        assert!(contacts(a, b, 1., [-2., 1., 0.], [4., 0., 0.], 1.)
            .unwrap()
            .is_empty());
        let inside = contacts(a, b, 1., [0.; 3], [0.; 3], 1.).unwrap();
        assert_eq!((inside[0].fraction, inside[1].fraction), (0., 1.));
        assert!(contacts(a, b, 1., [-2., 0., 0.], [4., 0., 0.], 0.2)
            .unwrap()
            .is_empty());
    }
}
