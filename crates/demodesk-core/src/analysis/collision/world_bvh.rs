//! Ordered scene traversal; every accepted hit can shorten subsequent visits.
use anyhow::{ensure, Result};
#[derive(Debug)]
pub struct Node {
    pub bounds: [[f32; 3]; 2],
    pub left: i32,
    pub right: i32,
    pub leaf: u64,
}
pub fn walk(
    root: i32,
    nodes: &[Node],
    start: [f32; 3],
    delta: [f32; 3],
    mut maximum: f32,
    allowance: f32,
    mut visit: impl FnMut(u64, f32) -> Result<f32>,
) -> Result<f32> {
    if root < 0 {
        return Ok(maximum);
    }
    let mut stack = vec![root];
    let mut visits = 0;
    while let Some(index) = stack.pop() {
        ensure!(index >= 0, "negative world child");
        let node = nodes
            .get(index as usize)
            .ok_or_else(|| anyhow::anyhow!("world node out of bounds"))?;
        visits += 1;
        ensure!(visits <= nodes.len(), "cyclic world tree");
        let endpoint = std::array::from_fn::<_, 3, _>(|i| start[i] + delta[i] * maximum);
        if (0..3).any(|i| {
            node.bounds[1][i] < start[i].min(endpoint[i])
                || start[i].max(endpoint[i]) < node.bounds[0][i]
        }) {
            continue;
        }
        let center =
            std::array::from_fn::<_, 3, _>(|i| (node.bounds[1][i] + node.bounds[0][i]) * 0.5);
        let ext = std::array::from_fn::<_, 3, _>(|i| node.bounds[1][i] - center[i]);
        let rel = std::array::from_fn::<_, 3, _>(|i| start[i] - center[i]);
        if (0..3).any(|i| {
            let j = (i + 1) % 3;
            let k = (i + 2) % 3;
            ((rel[k] * delta[j]) - (rel[j] * delta[k])).abs()
                - (ext[j] * delta[k].abs() + ext[k] * delta[j].abs())
                > 0.
        }) {
            continue;
        }
        if node.left != -1 {
            stack.push(node.right);
            stack.push(node.left);
            continue;
        }
        let next = (visit(node.leaf, maximum)? + allowance).min(1.);
        if next == 0. {
            return Ok(0.);
        }
        if next > 0. && maximum > next {
            maximum = next;
        }
    }
    Ok(maximum)
}
