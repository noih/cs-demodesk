//! Independently transcribed pinned tier0 FMA branch, finite |x| <= 7 radians only.
//! Domain excludes slow huge-angle reduction and exceptional float handling.
const fn d(x: u64) -> f64 {
    f64::from_bits(x)
}
fn ps(x: f64) -> f64 {
    let z = x * x;
    let p = z.mul_add(d(0x3ec71de3a556c734), d(0xbf2a01a01a01a01a));
    let p = p.mul_add(z, d(0x3f81111111111111));
    let p = p.mul_add(z, d(0xbfc5555555555555));
    p.mul_add(x * z, x)
}
fn pc(x: f64, fused_base: bool) -> f64 {
    let z = x * x;
    let base = if fused_base {
        z.mul_add(-0.5, 1.0)
    } else {
        1.0 - z * 0.5
    };
    let p = z.mul_add(d(0xbe927e4fb7789f5c), d(0x3efa01a01a01a019));
    let p = p.mul_add(z, d(0xbf56c16c16c16c16));
    let p = p.mul_add(z, d(0x3fa5555555555555));
    p.mul_add(z * z, base)
}
fn reduced(x: f64) -> (i32, f64) {
    let n = x.mul_add(d(0x3fe45f306dc9c883), 0.5).trunc() as i32;
    let high = (-(n as f64)).mul_add(d(0x3ff921fb54400000), x);
    let low = n as f64 * d(0x3dd0b4611a626331);
    (n, high - low)
}
pub fn sin(x: f32) -> f32 {
    assert!(x.is_finite() && x.abs() <= 7.0);
    let v = x as f64;
    let a = v.abs();
    if a <= d(0x3fe921fb54442d18) {
        return if a < d(0x3f20000000000000) {
            x
        } else if a < d(0x3f80000000000000) {
            (-(v * v * v)).mul_add(d(0x3fc5555555555555), v) as f32
        } else {
            ps(v) as f32
        };
    }
    let (n, r) = reduced(a);
    let mut y = if n & 1 == 0 { ps(r) } else { pc(r, true) };
    if (n & 2 != 0) ^ x.is_sign_negative() {
        y = -y;
    }
    y as f32
}
pub fn cos(x: f32) -> f32 {
    assert!(x.is_finite() && x.abs() <= 7.0);
    let v = x as f64;
    let a = v.abs();
    if a <= d(0x3fe921fb54442d18) {
        return if a < d(0x3f20000000000000) {
            1.0
        } else if a < d(0x3f80000000000000) {
            (-(v * 0.5)).mul_add(v, 1.0) as f32
        } else {
            pc(v, false) as f32
        };
    }
    let (n, r) = reduced(a);
    let mut y = if n & 1 == 0 { pc(r, false) } else { ps(r) };
    if ((n + 1) >> 1) & 1 != 0 {
        y = -y;
    }
    y as f32
}
