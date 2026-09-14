//! Rigid transform and one-sided nearest-query kernels.
pub(super) fn hull_contact(
    planes: &[[f32; 4]],
    a: [f32; 3],
    delta: [f32; 3],
) -> Option<(f32, bool)> {
    let (mut lo, mut hi) = (-f32::MAX, f32::MAX);
    let mut inside = true;
    for p in planes {
        let da = ((p[2] * a[2] + p[1] * a[1]) + p[0] * a[0]) - p[3];
        let db = ((p[2] * delta[2] + p[1] * delta[1]) + p[0] * delta[0]) + da;
        if da > 0. && db > 0. {
            return None;
        }
        if da <= 0. && db <= 0. {
            continue;
        }
        inside &= da <= 0.;
        if da > db {
            let t = (da - 0.03125) / (da - db);
            if t > lo {
                lo = t;
            }
        } else {
            let t = (da + 0.03125) / (da - db);
            if t < hi {
                hi = t;
            }
        }
    }
    (hi > lo).then_some((if inside { 0. } else { lo }, inside))
}
pub fn inverse_rotate([x, y, z, w]: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let xx = x * x;
    let yy = y * y;
    let zz = z * z;
    let xy = x * y;
    let xz = x * z;
    let yz = y * z;
    let wx = w * x;
    let wy = w * y;
    let wz = w * z;
    let twice = |a: f32| a + a;
    let m = [
        [1. - twice(zz + yy), twice(wz + xy), twice(xz - wy)],
        [twice(xy - wz), 1. - twice(zz + xx), twice(yz + wx)],
        [twice(wy + xz), twice(yz - wx), 1. - twice(yy + xx)],
    ];
    m.map(|r| (v[1] * r[1] + v[0] * r[0]) + v[2] * r[2])
}
pub(super) fn triangle_fraction_delta(t: [[f32; 3]; 3], a: [f32; 3], d: [f32; 3]) -> Option<f32> {
    let sub = |a: [f32; 3], b: [f32; 3]| std::array::from_fn(|i| a[i] - b[i]);
    let cross = |a: [f32; 3], b: [f32; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let dot = |a: [f32; 3], b: [f32; 3]| (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2];
    for (u, v) in [(t[1], t[2]), (t[2], t[0]), (t[0], t[1])] {
        let mid = std::array::from_fn(|i| (u[i] + v[i]) * 0.5 - a[i]);
        if dot(cross(sub(v, u), mid), d) < 0. {
            return None;
        }
    }
    let normal = cross(sub(t[1], t[0]), sub(t[2], t[0]));
    let denom = dot(normal, d);
    if denom >= 0. {
        return None;
    }
    let fraction = dot(sub(t[0], a), normal) / denom;
    (fraction > 0. && fraction < 1.).then_some(fraction)
}
