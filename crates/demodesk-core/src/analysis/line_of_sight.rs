//! Shared geometric line-of-sight queries, with an eye-direction front half-space.
//! A clear sample proves an exposed body point. Hidden requires whole-volume proof.
use anyhow::{ensure, Result};
pub type Vec3 = [f64; 3];
fn sub(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] - b[i])
}
fn add(a: Vec3, b: Vec3) -> Vec3 {
    std::array::from_fn(|i| a[i] + b[i])
}
fn mul(a: Vec3, b: f64) -> Vec3 {
    a.map(|x| x * b)
}
fn dot(a: Vec3, b: Vec3) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn finite(a: Vec3) -> bool {
    a.iter().all(|v| v.is_finite())
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occlusion {
    Clear,
    Blocked,
    Unknown,
}
#[derive(Clone, Debug)]
pub struct Triangle {
    pub vertices: [Vec3; 3],
    pub opaque: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct Capsule {
    pub a: Vec3,
    pub b: Vec3,
    pub radius: f64,
}
impl Capsule {
    pub fn valid(&self) -> bool {
        finite(self.a) && finite(self.b) && self.radius.is_finite() && self.radius > 0.
    }
}
#[derive(Clone, Copy)]
pub struct Bounds {
    pub min: Vec3,
    pub max: Vec3,
}
impl Bounds {
    fn triangles(triangles: &[Triangle]) -> Self {
        let mut b = Self {
            min: [f64::INFINITY; 3],
            max: [f64::NEG_INFINITY; 3],
        };
        for t in triangles {
            for p in t.vertices {
                for (i, value) in p.iter().enumerate() {
                    b.min[i] = b.min[i].min(*value);
                    b.max[i] = b.max[i].max(*value);
                }
            }
        }
        b
    }
    pub fn intersects(&self, start: Vec3, end: Vec3) -> bool {
        self.segment_interval(start, end).is_some()
    }
    pub fn segment_interval(&self, start: Vec3, end: Vec3) -> Option<[f64; 2]> {
        let mut lo: f64 = 0.;
        let mut hi: f64 = 1.;
        for i in 0..3 {
            let d = end[i] - start[i];
            if d == 0. {
                if start[i] < self.min[i] || start[i] > self.max[i] {
                    return None;
                }
            } else {
                let a = (self.min[i] - start[i]) / d;
                let b = (self.max[i] - start[i]) / d;
                lo = lo.max(a.min(b));
                hi = hi.min(a.max(b));
                if lo > hi {
                    return None;
                }
            }
        }
        Some([lo, hi])
    }
}
struct Node {
    bounds: Bounds,
    start: usize,
    end: usize,
    children: Option<(usize, usize)>,
}
pub struct World {
    triangles: Vec<Triangle>,
    nodes: Vec<Node>,
    neighbors: Vec<[Option<(usize, usize)>; 3]>,
}
fn intersection(start: Vec3, end: Vec3, t: &Triangle) -> bool {
    let [a, b, c] = t.vertices;
    let direction = sub(end, start);
    let e1 = sub(b, a);
    let e2 = sub(c, a);
    let h = cross(direction, e2);
    let determinant = dot(e1, h);
    if determinant.abs() < 1e-12 {
        return false;
    }
    let s = sub(start, a);
    let u = dot(s, h) / determinant;
    if !(0. ..=1.).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = dot(direction, q) / determinant;
    if v < 0. || u + v > 1. {
        return false;
    }
    let distance = dot(e2, q) / determinant;
    distance > 0. && distance < 1.
}
/// Whether one opaque triangle's shadow cone contains the entire capsule.
fn covers(eye: Vec3, capsule: Capsule, t: &Triangle) -> bool {
    covers_except_edge(eye, capsule, t, None)
}
fn covers_except_edge(eye: Vec3, capsule: Capsule, t: &Triangle, shared: Option<usize>) -> bool {
    if !t.opaque {
        return false;
    }
    let [a, b, c] = t.vertices;
    let normal = cross(sub(b, a), sub(c, a));
    let side = dot(normal, sub(eye, a));
    if side.abs() < 1e-12 {
        return false;
    }
    let n = mul(normal, side.signum());
    let margin = capsule.radius * dot(n, n).sqrt();
    if dot(n, sub(capsule.a, a)) + margin >= 0. || dot(n, sub(capsule.b, a)) + margin >= 0. {
        return false;
    }
    for (edge, (first, second, inside)) in [(a, b, c), (b, c, a), (c, a, b)].into_iter().enumerate()
    {
        if shared == Some(edge) {
            continue;
        }
        let n = cross(sub(first, eye), sub(second, eye));
        let sign = dot(n, sub(inside, eye)).signum();
        if sign == 0. {
            return false;
        }
        let n = mul(n, sign);
        let margin = capsule.radius * dot(n, n).sqrt();
        if dot(n, sub(capsule.a, eye)) < margin || dot(n, sub(capsule.b, eye)) < margin {
            return false;
        }
    }
    true
}
impl World {
    pub fn new(triangles: Vec<Triangle>) -> Result<Self> {
        ensure!(!triangles.is_empty(), "empty obstruction geometry");
        ensure!(
            triangles
                .iter()
                .all(|t| t.vertices.iter().all(|p| finite(*p))),
            "nonfinite obstruction geometry"
        );
        let mut world = Self {
            triangles,
            nodes: vec![],
            neighbors: vec![],
        };
        world.build(0, world.triangles.len());
        world.join_edges();
        Ok(world)
    }
    fn join_edges(&mut self) {
        let mut edges = std::collections::HashMap::new();
        self.neighbors = vec![[None; 3]; self.triangles.len()];
        for (index, triangle) in self.triangles.iter().enumerate().filter(|(_, t)| t.opaque) {
            for edge in 0..3 {
                // Exact shared vertices only: never bridge a real crack in the mesh.
                let key = |p: Vec3| p.map(|v| if v == 0. { 0 } else { v.to_bits() });
                let mut endpoints = [
                    key(triangle.vertices[edge]),
                    key(triangle.vertices[(edge + 1) % 3]),
                ];
                endpoints.sort_unstable();
                if let Some((other, other_edge)) = edges.remove(&endpoints) {
                    self.neighbors[index][edge] = Some((other, other_edge));
                    self.neighbors[other][other_edge] = Some((index, edge));
                } else {
                    edges.insert(endpoints, (index, edge));
                }
            }
        }
    }

    fn covers_joined(&self, index: usize, eye: Vec3, capsule: Capsule) -> bool {
        let triangle = &self.triangles[index];
        if covers(eye, capsule, triangle) {
            return true;
        }
        // Full coverage must also block the center ray through one of the pair.
        // The BVH will visit that face even if the cached face no longer does.
        if !triangle.opaque || !intersection(eye, mul(add(capsule.a, capsule.b), 0.5), triangle) {
            return false;
        }
        // ponytail: join one neighboring face at a time; larger mesh unions can
        // remain unknown until measured coverage justifies a general union solver.
        self.neighbors[index]
            .iter()
            .enumerate()
            .any(|(edge, neighbor)| {
                let Some((other, other_edge)) = *neighbor else {
                    return false;
                };
                let other = &self.triangles[other];
                let plane = cross(
                    sub(triangle.vertices[edge], eye),
                    sub(triangle.vertices[(edge + 1) % 3], eye),
                );
                let side = dot(plane, sub(triangle.vertices[(edge + 2) % 3], eye));
                let other_side = dot(plane, sub(other.vertices[(other_edge + 2) % 3], eye));
                // Opposing half-spaces cover the internal seam. Keep all four outer
                // shadow planes and both depth planes, so holes/folds are not filled.
                side * other_side < 0.
                    && covers_except_edge(eye, capsule, triangle, Some(edge))
                    && covers_except_edge(eye, capsule, other, Some(other_edge))
            })
    }
    fn build(&mut self, start: usize, end: usize) -> usize {
        let bounds = Bounds::triangles(&self.triangles[start..end]);
        let index = self.nodes.len();
        self.nodes.push(Node {
            bounds,
            start,
            end,
            children: None,
        });
        if end - start > 8 {
            let axis = (0..3)
                .max_by(|a, b| {
                    (bounds.max[*a] - bounds.min[*a]).total_cmp(&(bounds.max[*b] - bounds.min[*b]))
                })
                .unwrap_or(0);
            let mid = (end - start) / 2;
            self.triangles[start..end].select_nth_unstable_by(mid, |a, b| {
                a.vertices
                    .iter()
                    .map(|p| p[axis])
                    .sum::<f64>()
                    .total_cmp(&b.vertices.iter().map(|p| p[axis]).sum::<f64>())
            });
            let left = self.build(start, start + mid);
            let right = self.build(start + mid, end);
            self.nodes[index].children = Some((left, right));
        }
        index
    }
    pub fn ray(&self, start: Vec3, end: Vec3) -> Occlusion {
        if !finite(start) || !finite(end) || start == end {
            return Occlusion::Unknown;
        }
        self.ray_node(0, start, end)
    }
    fn ray_node(&self, index: usize, start: Vec3, end: Vec3) -> Occlusion {
        if self.first_hit(index, start, end, true).is_some() {
            Occlusion::Blocked
        } else if self.first_hit(index, start, end, false).is_some() {
            Occlusion::Unknown
        } else {
            Occlusion::Clear
        }
    }
    fn first_hit(&self, index: usize, start: Vec3, end: Vec3, opaque_only: bool) -> Option<usize> {
        let node = &self.nodes[index];
        if !node.bounds.intersects(start, end) {
            return None;
        }
        if let Some((left, right)) = node.children {
            return self
                .first_hit(left, start, end, opaque_only)
                .or_else(|| self.first_hit(right, start, end, opaque_only));
        }
        (node.start..node.end).find(|&i| {
            (!opaque_only || self.triangles[i].opaque)
                && intersection(start, end, &self.triangles[i])
        })
    }
    fn clear_cached(&self, start: Vec3, end: Vec3, cached: &mut Option<usize>) -> bool {
        if !finite(start) || !finite(end) || start == end {
            return false;
        }
        if cached
            .and_then(|i| self.triangles.get(i))
            .is_some_and(|t| intersection(start, end, t))
        {
            return false;
        }
        *cached = self.first_hit(0, start, end, false);
        cached.is_none()
    }
    fn covered(&self, index: usize, eye: Vec3, c: Capsule) -> Option<usize> {
        let node = &self.nodes[index];
        let center = mul(add(c.a, c.b), 0.5);
        if !node.bounds.intersects(eye, center) {
            return None;
        }
        if let Some((left, right)) = node.children {
            return self
                .covered(left, eye, c)
                .or_else(|| self.covered(right, eye, c));
        }
        (node.start..node.end).find(|&i| self.covers_joined(i, eye, c))
    }
    fn covered_cached(&self, eye: Vec3, c: Capsule, cached: &mut Option<usize>) -> bool {
        if cached.is_some_and(|i| i < self.triangles.len() && self.covers_joined(i, eye, c)) {
            return true;
        }
        *cached = self.covered(0, eye, c);
        cached.is_some()
    }
    pub fn body(&self, eye: Vec3, forward: Vec3, body: &[Capsule]) -> Occlusion {
        self.body_with_clearance(eye, forward, body, true, &mut vec![], |_, _| true)
    }
    pub fn body_with_clearance(
        &self,
        eye: Vec3,
        forward: Vec3,
        body: &[Capsule],
        witnesses_possible: bool,
        wall_cache: &mut Vec<Option<usize>>,
        mut clear: impl FnMut(Vec3, Vec3) -> bool,
    ) -> Occlusion {
        if !finite(eye)
            || !finite(forward)
            || dot(forward, forward) < 1e-12
            || body.is_empty()
            || body.iter().any(|c| !c.valid())
        {
            return Occlusion::Unknown;
        }
        let forward = mul(forward, 1. / dot(forward, forward).sqrt());
        wall_cache.resize(body.len() * 22 + 1, None);
        // One enclosing-volume proof avoids repeating the same wall query for every hitbox.
        let min: Vec3 = std::array::from_fn(|i| {
            body.iter()
                .map(|c| c.a[i].min(c.b[i]))
                .fold(f64::INFINITY, f64::min)
        });
        let max: Vec3 = std::array::from_fn(|i| {
            body.iter()
                .map(|c| c.a[i].max(c.b[i]))
                .fold(f64::NEG_INFINITY, f64::max)
        });
        let center = mul(add(min, max), 0.5);
        let radius = body
            .iter()
            .flat_map(|c| {
                [c.a, c.b].map(|point| {
                    let d = sub(point, center);
                    dot(d, d).sqrt() + c.radius
                })
            })
            .fold(0., f64::max);
        if dot(sub(center, eye), forward) + radius <= 0.
            || self.covered_cached(
                eye,
                Capsule {
                    a: center,
                    b: center,
                    radius,
                },
                &mut wall_cache[body.len() * 22],
            )
        {
            return Occlusion::Blocked;
        }
        let mut all_hidden = true;
        for (index, &c) in body.iter().enumerate() {
            // The complete capsule is behind the eye plane; no aspect ratio is needed.
            if dot(sub(c.a, eye), forward).max(dot(sub(c.b, eye), forward)) + c.radius <= 0. {
                continue;
            }
            if self.covered_cached(eye, c, &mut wall_cache[index * 22]) {
                continue;
            }
            all_hidden = false;
            if !witnesses_possible {
                return Occlusion::Unknown;
            }
            // Witness rays never certify hidden: unsampled slivers remain unknown.
            let toward = mul(add(c.a, c.b), 0.5);
            let direction = sub(eye, toward);
            let length = dot(direction, direction).sqrt();
            if length <= c.radius {
                return Occlusion::Unknown;
            }
            let toward_eye = mul(direction, 1. / length);
            for (center_index, center) in [c.a, c.b, toward].into_iter().enumerate() {
                for (axis_index, axis) in [
                    toward_eye,
                    [1., 0., 0.],
                    [-1., 0., 0.],
                    [0., 1., 0.],
                    [0., -1., 0.],
                    [0., 0., 1.],
                    [0., 0., -1.],
                ]
                .into_iter()
                .enumerate()
                {
                    let point = add(center, mul(axis, c.radius));
                    if dot(sub(point, eye), forward) > 0.
                        && self.clear_cached(
                            eye,
                            point,
                            &mut wall_cache[index * 22 + 1 + center_index * 7 + axis_index],
                        )
                        && clear(eye, point)
                    {
                        return Occlusion::Clear;
                    }
                }
            }
        }
        if all_hidden {
            Occlusion::Blocked
        } else {
            Occlusion::Unknown
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn joined_wall_triangles_hide_a_body_across_their_seam() {
        let triangles = vec![
            Triangle {
                vertices: [[5., -10., -10.], [5., 10., -10.], [5., 10., 10.]],
                opaque: true,
            },
            Triangle {
                vertices: [[5., -10., -10.], [5., 10., 10.], [5., -10., 10.]],
                opaque: true,
            },
        ];
        let c = Capsule {
            a: [10., 0., 0.],
            b: [10., 0., 1.],
            radius: 0.5,
        };
        assert!(triangles.iter().all(|t| !covers([0.; 3], c, t)));
        let world = World::new(triangles.clone()).unwrap();
        assert_eq!(world.body([0.; 3], [1., 0., 0.], &[c]), Occlusion::Blocked);
        let mut cache = vec![];
        for _ in 0..2 {
            assert_eq!(
                world.body_with_clearance([0.; 3], [1., 0., 0.], &[c], true, &mut cache, |_, _| {
                    true
                }),
                Occlusion::Blocked
            );
        }
        assert_eq!(
            world.body_with_clearance(
                [20., 0., 0.],
                [-1., 0., 0.],
                &[c],
                true,
                &mut cache,
                |_, _| true
            ),
            Occlusion::Clear
        );
        let mut transparent = triangles;
        transparent[1].opaque = false;
        assert_eq!(
            World::new(transparent)
                .unwrap()
                .body([0.; 3], [1., 0., 0.], &[c]),
            Occlusion::Unknown
        );
    }

    #[test]
    fn joined_shadows_do_not_close_cracks_or_hide_exposed_points() {
        let c = Capsule {
            a: [10., 0., 0.],
            b: [10., 0., 1.],
            radius: 0.5,
        };
        for gap in [0., 0.01] {
            for fold in [0., -2., 2.] {
                let triangles = vec![
                    Triangle {
                        vertices: [[5., -10., -10.], [5., 10., -10.], [5., 10., 10.]],
                        opaque: true,
                    },
                    Triangle {
                        vertices: [
                            [5., 10. - gap, 10. + gap],
                            [5., -10. - gap, -10. + gap],
                            [5. + fold, -10., 10.],
                        ],
                        opaque: true,
                    },
                ];
                let world = World::new(triangles).unwrap();
                if gap > 0. {
                    assert!(world.neighbors.iter().flatten().all(Option::is_none));
                } else {
                    assert_eq!(world.body([0.; 3], [1., 0., 0.], &[c]), Occlusion::Blocked);
                }
                for y in -12..=12 {
                    let eye = [0., f64::from(y), 0.];
                    if world.body(eye, [1., 0., 0.], &[c]) != Occlusion::Blocked {
                        continue;
                    }
                    // Independently check surface rays against the original triangles.
                    for z in [0., 0.5, 1.] {
                        for i in 0..64 {
                            let angle = f64::from(i) * std::f64::consts::TAU / 64.;
                            let point = [10., c.radius * angle.cos(), z + c.radius * angle.sin()];
                            assert_eq!(
                                world.ray(eye, point),
                                Occlusion::Blocked,
                                "gap={gap}, fold={fold}, eye={eye:?}, point={point:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn body_edges_unknown_material_and_whole_body_occlusion() {
        let triangle = Triangle {
            vertices: [[5., -100., -100.], [5., 100., -100.], [5., 0., 100.]],
            opaque: true,
        };
        let world = World::new(vec![triangle.clone()]).unwrap();
        let c = Capsule {
            a: [10., 0., 0.],
            b: [10., 0., 1.],
            radius: 0.5,
        };
        assert_eq!(world.body([0.; 3], [1., 0., 0.], &[c]), Occlusion::Blocked);
        assert_eq!(
            world.body([20., 0., 0.], [-1., 0., 0.], &[c]),
            Occlusion::Clear
        ); // Eye direction faces the target.
        assert_eq!(
            world.body_with_clearance(
                [0.; 3],
                [1., 0., 0.],
                &[c],
                false,
                &mut vec![],
                |_, _| panic!("unexpected witness")
            ),
            Occlusion::Blocked
        );
        assert_eq!(
            world.body_with_clearance(
                [20., 0., 0.],
                [-1., 0., 0.],
                &[c],
                false,
                &mut vec![],
                |_, _| panic!("unexpected witness")
            ),
            Occlusion::Unknown
        );
        let mut cache = vec![Some(usize::MAX)];
        assert_eq!(
            world.body_with_clearance([0.; 3], [1., 0., 0.], &[c], true, &mut cache, |_, _| true),
            Occlusion::Blocked
        );
        assert_eq!(
            world.body_with_clearance(
                [20., 0., 0.],
                [-1., 0., 0.],
                &[c],
                true,
                &mut cache,
                |_, _| true
            ),
            Occlusion::Clear
        );
        let mut ray_cache = None;
        assert!(!world.clear_cached([0., 0., 0.], [10., 0., 0.], &mut ray_cache));
        assert!(world.clear_cached([20., 0., 0.], [10., 0., 0.], &mut ray_cache));
        let mut transparent = triangle;
        transparent.opaque = false;
        assert_eq!(
            World::new(vec![transparent])
                .unwrap()
                .body([0.; 3], [1., 0., 0.], &[c]),
            Occlusion::Unknown
        );
        let wall = World::new(vec![Triangle {
            vertices: [[5., -10., -10.], [5., 10., -10.], [5., 0., 0.]],
            opaque: true,
        }])
        .unwrap();
        let edge = Capsule {
            a: [10., 0., -0.2],
            b: [10., 0., -0.1],
            radius: 1.,
        };
        assert_eq!(wall.ray([0.; 3], edge.a), Occlusion::Blocked);
        assert_eq!(wall.body([0.; 3], [1., 0., 0.], &[edge]), Occlusion::Clear);
        assert_eq!(wall.body([0.; 3], [1., 0., 0.], &[]), Occlusion::Unknown);
        assert_eq!(
            wall.body([0.; 3], [-1., 0., 0.], &[edge]),
            Occlusion::Blocked
        );
        let straddling = Capsule {
            a: [-0.2, 10., 0.],
            b: [-0.1, 10., 0.],
            radius: 1.,
        };
        assert_eq!(
            wall.body([0.; 3], [1., 0., 0.], &[straddling]),
            Occlusion::Clear
        );
    }
    #[test]
    fn bvh_matches_direct_segment_queries() {
        let triangles: Vec<_> = (1..50)
            .map(|x| Triangle {
                vertices: [
                    [f64::from(x), -1., -1.],
                    [f64::from(x), 1., -1.],
                    [f64::from(x), 0., 1.],
                ],
                opaque: true,
            })
            .collect();
        let world = World::new(triangles.clone()).unwrap();
        for y in -20..20 {
            let eye = [0., f64::from(y) / 10., 0.];
            let end = [60., f64::from(y) / 10., 0.];
            assert_eq!(
                world.ray(eye, end) == Occlusion::Blocked,
                triangles.iter().any(|t| intersection(eye, end, t))
            );
        }
    }
}
