//! Ordered mesh traversal with a shrinking finite query bound.
use super::super as ballistic_primitives;
use anyhow::{ensure, Result};
#[derive(Clone)]
pub struct Node {
    pub bounds: [[f32; 3]; 2],
    pub descriptor: u32,
    pub first: u32,
}
pub fn collect(
    nodes: &[Node],
    triangles: &[[[f32; 3]; 3]],
    start: [f32; 3],
    delta: [f32; 3],
    maximum: f32,
    allowance: f32,
) -> Result<Vec<(usize, f32, bool)>> {
    ensure!(!nodes.is_empty(), "empty mesh tree");
    let mut stack = vec![0usize];
    let mut limit = 1f32;
    let mut out = vec![];
    let mut visits = 0;
    while let Some(index) = stack.pop() {
        visits += 1;
        ensure!(visits <= nodes.len(), "cyclic mesh tree");
        let n = nodes
            .get(index)
            .ok_or_else(|| anyhow::anyhow!("invalid node"))?;
        let end = std::array::from_fn::<_, 3, _>(|i| start[i] + delta[i] * limit);
        let lo = std::array::from_fn::<_, 3, _>(|i| start[i].min(end[i]));
        let hi = std::array::from_fn::<_, 3, _>(|i| start[i].max(end[i]));
        if (0..3).any(|i| n.bounds[1][i] < lo[i] || hi[i] < n.bounds[0][i]) {
            continue;
        }
        let center = std::array::from_fn::<_, 3, _>(|i| (n.bounds[1][i] + n.bounds[0][i]) * 0.5);
        let extent = std::array::from_fn::<_, 3, _>(|i| n.bounds[1][i] - center[i]);
        let relative = std::array::from_fn::<_, 3, _>(|i| start[i] - center[i]);
        if (0..3).any(|i| {
            let j = (i + 1) % 3;
            let k = (i + 2) % 3;
            ((relative[k] * delta[j]) - (relative[j] * delta[k])).abs()
                > (extent[j] * delta[k].abs() + extent[k] * delta[j].abs())
        }) {
            continue;
        }
        let axis = (n.descriptor >> 30) as usize;
        if axis == 3 {
            let count = (n.descriptor & 0x3fffffff) as usize;
            for t in n.first as usize..n.first as usize + count {
                let tri = *triangles
                    .get(t)
                    .ok_or_else(|| anyhow::anyhow!("invalid triangle"))?;
                let end = std::array::from_fn::<_, 3, _>(|i| start[i] + delta[i] * limit);
                if (0..3).any(|i| {
                    let low = tri[0][i].min(tri[1][i].min(tri[2][i])) - 0.03125;
                    let high = tri[0][i].max(tri[1][i].max(tri[2][i])) + 0.03125;
                    start[i].max(end[i]) < low || high < start[i].min(end[i])
                }) {
                    continue;
                }
                if let Some((fraction, exit)) =
                    ballistic_primitives::triangle(tri, start, delta, maximum)
                {
                    out.push((t, fraction, exit));
                    let proposed = fraction + allowance;
                    if limit > proposed {
                        limit = proposed;
                    }
                }
            }
        } else {
            let second = index + (n.descriptor & 0x3fffffff) as usize;
            let first = index + 1;
            ensure!(second < nodes.len() && first < nodes.len(), "invalid child");
            if delta[axis] > 0. {
                stack.push(second);
                stack.push(first);
            } else {
                stack.push(first);
                stack.push(second);
            }
        }
    }
    out.sort_by(|a, b| a.1.total_cmp(&b.1));
    Ok(out)
}
