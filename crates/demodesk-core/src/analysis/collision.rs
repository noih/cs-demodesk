//! Source 2 collision primitives. Floating-point order matches the qualified native queries.
//! World filtering, transforms and mesh candidate consolidation are separate.
use anyhow::{ensure, Result};
#[derive(Clone, Copy, Debug)]
pub struct Contact {
    pub fraction: f32,
    pub normal: [f32; 3],
    pub exit: bool,
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}
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
fn clip(
    planes: &[[f32; 4]],
    start: [f32; 3],
    delta: [f32; 3],
) -> Option<(f32, f32, Option<usize>, Option<usize>)> {
    let (mut lo, mut hi, mut entering, mut leaving) = (-f32::MAX, f32::MAX, None, None);
    for (i, p) in planes.iter().enumerate() {
        let a = ((p[2] * start[2] + p[1] * start[1]) + p[0] * start[0]) - p[3];
        let b = ((p[2] * delta[2] + p[1] * delta[1]) + p[0] * delta[0]) + a;
        if a > 0. && b > 0. {
            return None;
        }
        if a <= 0. && b <= 0. {
            continue;
        }
        if a > b {
            let t = (a - 0.03125) / (a - b);
            if t > lo {
                lo = t;
                entering = Some(i);
            }
        } else {
            let t = (a + 0.03125) / (a - b);
            if t < hi {
                hi = t;
                leaving = Some(i);
            }
        }
    }
    (hi > lo).then_some((lo, hi, entering, leaving))
}
fn nearest(planes: &[[f32; 4]], start: [f32; 3], delta: [f32; 3], scale: f32) -> Option<Contact> {
    let inv = 1. / scale;
    let (lo, _, entry, _) = clip(planes, start.map(|x| x * inv), delta.map(|x| x * inv))?;
    match entry {
        Some(i) if lo <= 1. => Some(Contact {
            fraction: lo,
            normal: [planes[i][0], planes[i][1], planes[i][2]],
            exit: false,
        }),
        None => {
            let length = ((delta[1] * delta[1] + delta[2] * delta[2]) + delta[0] * delta[0]).sqrt();
            Some(Contact {
                fraction: 0.,
                normal: delta.map(|x| -x * (1. / length)),
                exit: false,
            })
        }
        _ => None,
    }
}
pub fn hull(
    planes: &[[f32; 4]],
    start: [f32; 3],
    delta: [f32; 3],
    scale: f32,
    max_fraction: f32,
) -> Result<Vec<Contact>> {
    ensure!(
        !planes.is_empty()
            && planes
                .iter()
                .flatten()
                .chain(start.iter())
                .chain(delta.iter())
                .all(|v| v.is_finite()),
        "invalid hull/ray"
    );
    ensure!(
        scale.is_finite()
            && scale > 0.
            && max_fraction.is_finite()
            && max_fraction >= 0.
            && delta != [0.; 3],
        "invalid ray scale/range"
    );
    if let Some((lo, hi, Some(a), Some(b))) = clip(planes, start, delta) {
        if lo < max_fraction && hi > lo {
            return Ok(vec![
                Contact {
                    fraction: lo,
                    normal: [planes[a][0], planes[a][1], planes[a][2]],
                    exit: false,
                },
                Contact {
                    fraction: hi,
                    normal: [planes[b][0], planes[b][1], planes[b][2]],
                    exit: true,
                },
            ]);
        }
    }
    let Some(first) = nearest(planes, start, delta, scale).filter(|h| h.fraction <= max_fraction)
    else {
        return Ok(vec![]);
    };
    let Some(last) = nearest(
        planes,
        std::array::from_fn(|i| start[i] + delta[i]),
        delta.map(|x| -x),
        scale,
    ) else {
        return Ok(vec![]);
    };
    let end = 1. - last.fraction;
    Ok(if end > first.fraction {
        vec![
            first,
            Contact {
                fraction: end,
                normal: last.normal,
                exit: true,
            },
        ]
    } else {
        vec![]
    })
}
/// 2296F5..2298D7: two-sided triangle candidate; mesh consolidation not included.
pub fn triangle(
    vertices: [[f32; 3]; 3],
    start: [f32; 3],
    delta: [f32; 3],
    max_fraction: f32,
) -> Option<(f32, bool)> {
    let [a, b, c] = vertices;
    let edges = [sub(c, b), sub(a, c), sub(b, a)];
    let midpoints = [
        std::array::from_fn(|i| (c[i] + b[i]) * 0.5 - start[i]),
        std::array::from_fn(|i| (c[i] + a[i]) * 0.5 - start[i]),
        std::array::from_fn(|i| (b[i] + a[i]) * 0.5 - start[i]),
    ];
    let side = std::array::from_fn::<_, 3, _>(|i| dot(cross(edges[i], midpoints[i]), delta));
    if side.iter().any(|x| *x < 0.) && side.iter().any(|x| *x > 0.) {
        return None;
    }
    let normal = cross(sub(b, a), sub(c, a));
    let denominator = dot(normal, delta);
    let t = dot(sub(a, start), normal) / denominator;
    if !t.is_finite() || t <= 0. || t >= 1. || t > max_fraction {
        return None;
    }
    Some((t, denominator >= 0.))
}

/// 22DFE0 ordinary (unscaled-normal) branch.
pub fn triangle_normal(vertices: [[f32; 3]; 3]) -> [f32; 3] {
    let n = cross(sub(vertices[1], vertices[0]), sub(vertices[2], vertices[0]));
    let square = (n[1] * n[1] + n[0] * n[0]) + n[2] * n[2];
    if square <= 1.1754943508222875e-35 {
        return [0.; 3];
    }
    let inv = 1. / square.sqrt();
    n.map(|x| inv * x)
}

/// 22B4B0 chooses the nearest mesh AABB face, then tests the first surface orientation.
/// Caller supplies native traversal order for exact equal-distance ties.
pub fn mesh_inside(
    triangles: &[[[f32; 3]; 3]],
    bounds: [[f32; 3]; 2],
    point: [f32; 3],
    scale: [f32; 3],
    flags: u32,
) -> bool {
    if flags & 2 != 0 {
        return false;
    }
    let low = bounds[0].map(|x| x);
    let high = bounds[1];
    let low = std::array::from_fn::<_, 3, _>(|i| low[i] * scale[i]);
    let high = std::array::from_fn::<_, 3, _>(|i| high[i] * scale[i]);
    if (0..3).any(|i| point[i] < low[i] || point[i] > high[i]) {
        return false;
    }
    let lower = sub(point, low);
    let upper = sub(high, point);
    let choose = |v: [f32; 3]| {
        if v[1].abs() > v[0].abs() {
            if v[2].abs() > v[0].abs() {
                0
            } else {
                2
            }
        } else if v[2].abs() > v[1].abs() {
            1
        } else {
            2
        }
    };
    let a = choose(lower);
    let b = choose(upper);
    let mut delta = [0.; 3];
    if upper[b] > lower[a] {
        delta[a] = -lower[a];
    } else {
        delta[b] = upper[b];
    }
    let local = std::array::from_fn(|i| point[i] / scale[i]);
    let local_delta = std::array::from_fn(|i| delta[i] / scale[i]);
    let mut best = 1.;
    let mut normal = None;
    for vertices in triangles {
        if let Some((fraction, _)) = triangle(*vertices, local, local_delta, best) {
            if fraction < best {
                best = fraction;
                let e = sub(vertices[1], vertices[0]);
                let f = sub(vertices[2], vertices[0]);
                let n = cross(
                    std::array::from_fn(|i| e[i] * scale[i]),
                    std::array::from_fn(|i| f[i] * scale[i]),
                );
                let inv = 1. / ((n[2] * n[2] + n[1] * n[1]) + n[0] * n[0]).sqrt();
                normal = Some(n.map(|x| x * inv));
            }
        }
    }
    normal.is_some_and(|n| (delta[1] * n[1] + delta[0] * n[0]) + delta[2] * n[2] >= 0.)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_hull_and_triangle_boundaries() {
        let planes = [
            [1., 0., 0., 1.],
            [-1., 0., 0., 1.],
            [0., 1., 0., 1.],
            [0., -1., 0., 1.],
            [0., 0., 1., 1.],
            [0., 0., -1., 1.],
        ];
        let contacts = hull(&planes, [-5., 0., 0.], [10., 0., 0.], 1., 1.).unwrap();
        assert_eq!(contacts.len(), 2);
        assert_eq!(contacts[0].fraction.to_bits(), 0.396875_f32.to_bits());
        assert_eq!(contacts[1].fraction.to_bits(), 0.596875_f32.to_bits());
        assert_eq!(contacts[0].normal, [-1., 0., 0.]);
        assert!(!contacts[0].exit && contacts[1].exit);
        // Native fast hull clipping uses the unscaled planes; preserve that branch.
        assert_eq!(
            hull(&planes, [-5., 0., 0.], [10., 0., 0.], 2., 1.).unwrap()[0]
                .fraction
                .to_bits(),
            contacts[0].fraction.to_bits()
        );
        assert!(hull(&planes, [0.; 3], [0.; 3], 1., 1.).is_err());
        let vertices = [[-1., -1., 0.], [1., -1., 0.], [0., 1., 0.]];
        assert_eq!(
            triangle(vertices, [0., 0., -1.], [0., 0., 2.], 1.),
            Some((0.5, true))
        );
        assert_eq!(triangle(vertices, [0., 0., -1.], [0., 0., 1.], 1.), None);
        assert_eq!(triangle_normal(vertices), [0., 0., 1.]);
    }
}

pub mod asset;

pub mod filter;

pub mod capsule;
