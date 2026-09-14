//! Deterministic surface-area tree construction in PHYS resource order.
#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub min: [f32; 3],
    pub max: [f32; 3],
}
impl Bounds {
    fn union(self, b: Self) -> Self {
        Self {
            min: std::array::from_fn(|i| {
                if self.min[i] < b.min[i] {
                    self.min[i]
                } else {
                    b.min[i]
                }
            }),
            max: std::array::from_fn(|i| {
                if self.max[i] > b.max[i] {
                    self.max[i]
                } else {
                    b.max[i]
                }
            }),
        }
    }
    fn area(self) -> f32 {
        let [x, y, z] = std::array::from_fn(|i| self.max[i] - self.min[i]);
        let a = (z * y) + (y * x);
        let a = a + (z * x);
        a + a
    }
    fn center(self) -> [f32; 3] {
        std::array::from_fn(|i| (self.min[i] + self.max[i]) * 0.5)
    }
}
#[derive(Clone, Debug)]
pub struct Node {
    pub bounds: Bounds,
    pub left: i32,
    pub right: i32,
    pub parent: i32,
    pub height: i32,
    pub leaf: Option<usize>,
}
#[derive(Default)]
pub struct Tree {
    pub nodes: Vec<Node>,
    pub root: Option<usize>,
}
impl Tree {
    pub fn build(bounds: &[Bounds]) -> Result<Self, String> {
        let mut t = Self::default();
        for (i, b) in bounds.iter().enumerate() {
            if !(0..3).all(|a| b.min[a].is_finite() && b.max[a].is_finite() && b.min[a] <= b.max[a])
            {
                return Err(format!("invalid bounds {i}"));
            }
            t.insert(*b, i);
        }
        Ok(t)
    }
    fn refit(&mut self, n: usize) {
        let (l, r) = (self.nodes[n].left as usize, self.nodes[n].right as usize);
        self.nodes[n].bounds = self.nodes[l].bounds.union(self.nodes[r].bounds);
        self.nodes[n].height = 1 + self.nodes[l].height.max(self.nodes[r].height);
    }
    fn sibling(&self, b: Bounds) -> usize {
        let mut cur = self.root.unwrap();
        let mut best = cur;
        let mut best_cost = self.nodes[cur].bounds.union(b).area();
        let mut inherited = 0f32;
        let leaf_area = b.area();
        let center = b.center();
        while self.nodes[cur].left != -1 {
            let node = &self.nodes[cur];
            let union = node.bounds.union(b).area();
            let cost = inherited + union;
            if cost < best_cost {
                best_cost = cost;
                best = cur;
            }
            inherited += union - node.bounds.area();
            let [l, r] = [node.left as usize, node.right as usize];
            let mut lower = [f32::MAX; 2];
            for (slot, child) in [l, r].into_iter().enumerate() {
                let n = &self.nodes[child];
                let cost = n.bounds.union(b).area() + inherited;
                if n.left == -1 {
                    if cost < best_cost {
                        best_cost = cost;
                        best = child;
                    }
                } else {
                    lower[slot] = (leaf_area - n.bounds.area()).min(0.) + cost;
                }
            }
            if self.nodes[l].left == -1 && self.nodes[r].left == -1
                || lower[0] >= best_cost && lower[1] >= best_cost
            {
                break;
            }
            if lower[0] == lower[1] && self.nodes[l].left != -1 {
                for (i, n) in [l, r].into_iter().enumerate() {
                    let c = self.nodes[n].bounds.center();
                    let d: [f32; 3] = std::array::from_fn(|a| c[a] - center[a]);
                    lower[i] = (d[1] * d[1] + d[0] * d[0]) + d[2] * d[2];
                }
            }
            cur = if lower[0] < lower[1] && self.nodes[l].left != -1 {
                l
            } else {
                r
            };
            if self.nodes[cur].left == -1 {
                break;
            }
        }
        best
    }
    fn swap_child(&mut self, root: usize, root_side: bool, branch: usize, child_side: bool) {
        let outside = if root_side {
            self.nodes[root].right
        } else {
            self.nodes[root].left
        };
        let child = if child_side {
            self.nodes[branch].right
        } else {
            self.nodes[branch].left
        };
        if root_side {
            self.nodes[root].right = child
        } else {
            self.nodes[root].left = child
        }
        if child_side {
            self.nodes[branch].right = outside
        } else {
            self.nodes[branch].left = outside
        }
        self.nodes[outside as usize].parent = branch as i32;
        self.nodes[child as usize].parent = root as i32;
        self.refit(branch);
        self.refit(root);
    }
    fn rotate(&mut self, n: usize) {
        if self.nodes[n].height < 2 {
            return;
        }
        let (b, c) = (self.nodes[n].left as usize, self.nodes[n].right as usize);
        if self.nodes[b].height == 0 {
            let (f, g) = (self.nodes[c].left as usize, self.nodes[c].right as usize);
            let old = self.nodes[c].bounds.area();
            let a = self.nodes[b].bounds.union(self.nodes[g].bounds).area();
            let z = self.nodes[b].bounds.union(self.nodes[f].bounds).area();
            if a > old && z > old {
                return;
            }
            self.swap_child(n, false, c, z <= a);
            return;
        }
        if self.nodes[c].height == 0 {
            let (d, e) = (self.nodes[b].left as usize, self.nodes[b].right as usize);
            let old = self.nodes[b].bounds.area();
            let a = self.nodes[c].bounds.union(self.nodes[e].bounds).area();
            let z = self.nodes[c].bounds.union(self.nodes[d].bounds).area();
            if a > old && z > old {
                return;
            }
            self.swap_child(n, true, b, z <= a);
            return;
        }
        let (d, e, f, g) = (
            self.nodes[b].left as usize,
            self.nodes[b].right as usize,
            self.nodes[c].left as usize,
            self.nodes[c].right as usize,
        );
        let ab = self.nodes[b].bounds.area();
        let ac = self.nodes[c].bounds.area();
        let mut best = ab + ac;
        let mut choice = 0;
        let costs = [
            self.nodes[b].bounds.union(self.nodes[g].bounds).area() + ab,
            self.nodes[b].bounds.union(self.nodes[f].bounds).area() + ab,
            self.nodes[c].bounds.union(self.nodes[e].bounds).area() + ac,
            self.nodes[c].bounds.union(self.nodes[d].bounds).area() + ac,
        ];
        for (i, cost) in costs.into_iter().enumerate() {
            if cost < best {
                best = cost;
                choice = i + 1
            }
        }
        match choice {
            1 => self.swap_child(n, false, c, false),
            2 => self.swap_child(n, false, c, true),
            3 => self.swap_child(n, true, b, false),
            4 => self.swap_child(n, true, b, true),
            _ => {}
        }
    }
    fn insert(&mut self, bounds: Bounds, leaf: usize) {
        let n = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            left: -1,
            right: -1,
            parent: -1,
            height: 0,
            leaf: Some(leaf),
        });
        if self.root.is_none() {
            self.root = Some(n);
            return;
        }
        let sibling = self.sibling(bounds);
        let old = self.nodes[sibling].parent;
        let p = self.nodes.len();
        self.nodes.push(Node {
            bounds: bounds.union(self.nodes[sibling].bounds),
            left: sibling as i32,
            right: n as i32,
            parent: old,
            height: self.nodes[sibling].height + 1,
            leaf: None,
        });
        if old < 0 {
            self.root = Some(p)
        } else if self.nodes[old as usize].left == sibling as i32 {
            self.nodes[old as usize].left = p as i32
        } else {
            self.nodes[old as usize].right = p as i32
        }
        self.nodes[sibling].parent = p as i32;
        self.nodes[n].parent = p as i32;
        let mut cur = p as i32;
        while cur >= 0 {
            self.refit(cur as usize);
            self.rotate(cur as usize);
            cur = self.nodes[cur as usize].parent;
        }
    }
}
