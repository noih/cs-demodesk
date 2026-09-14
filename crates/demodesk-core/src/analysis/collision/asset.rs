//! Static PHYS geometry, decoded once and queried in engine tree order.
use super::{hull, mesh_inside, triangle_normal, Contact};
use crate::analysis::kv3_text::BinaryDocument;
use anyhow::{ensure, Context, Result};
use serde_json::Value;
#[path = "mesh_bvh.rs"]
mod mesh_bvh;
#[path = "tree_builder.rs"]
mod tree_builder;
#[path = "world_bvh.rs"]
mod world_bvh;

#[path = "nearest.rs"]
mod nearest;

#[derive(Clone, Copy, Debug)]
pub struct RigidTransform {
    pub origin: [f32; 3],
    pub rotation: [f32; 4],
    pub scale: f32,
}
impl RigidTransform {
    pub const IDENTITY: Self = Self {
        origin: [0.; 3],
        rotation: [0., 0., 0., 1.],
        scale: 1.,
    };
    fn local_ray(self, start: [f32; 3], delta: [f32; 3]) -> Result<([f32; 3], [f32; 3])> {
        ensure!(
            self.origin
                .iter()
                .chain(self.rotation.iter())
                .chain(start.iter())
                .chain(delta.iter())
                .all(|v| v.is_finite())
                && self.scale.is_finite()
                && self.scale > 0.,
            "invalid rigid transform/ray"
        );
        Ok((
            nearest::inverse_rotate(
                self.rotation,
                std::array::from_fn(|i| start[i] - self.origin[i]),
            ),
            nearest::inverse_rotate(self.rotation, delta),
        ))
    }
}
#[derive(Debug, Clone, Copy)]
pub struct NearestHit {
    pub fraction: f32,
    pub start_solid: bool,
    pub entity: u32,
    pub part: usize,
    pub shape: usize,
    pub attribute: usize,
    pub surface: usize,
}

pub struct Asset {
    pub attributes: Vec<Value>,
    pub surface_hashes: Vec<u32>,
    shapes: Vec<Shape>,
    nodes: Vec<world_bvh::Node>,
    root: i32,
}
struct Shape {
    part: usize,
    index: usize,
    attribute: usize,
    surface: usize,
    geometry: Geometry,
}
enum Geometry {
    Hull(Vec<[f32; 4]>),
    Mesh {
        triangles: Vec<[[f32; 3]; 3]>,
        nodes: Vec<mesh_bvh::Node>,
        bounds: [[f32; 3]; 2],
        flags: u32,
        materials: Vec<u8>,
    },
}
#[derive(Debug)]
pub struct Hit {
    pub contact: Contact,
    pub part: usize,
    pub shape: usize,
    pub attribute: usize,
    pub triangle: Option<usize>,
    pub surface: usize,
}
fn list<'a>(v: &'a Value, key: &str) -> Result<&'a [Value]> {
    Ok(v[key]
        .as_array()
        .with_context(|| format!("missing PHYS {key}"))?)
}
fn index(v: &Value, key: &str) -> Result<usize> {
    Ok(usize::try_from(
        v[key]
            .as_u64()
            .with_context(|| format!("invalid PHYS {key}"))?,
    )?)
}
fn floats<const N: usize>(bytes: &[u8]) -> Result<Vec<[f32; N]>> {
    ensure!(bytes.len() % (N * 4) == 0, "truncated PHYS float buffer");
    bytes
        .chunks_exact(N * 4)
        .map(|b| {
            let a = std::array::from_fn(|i| {
                f32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().expect("fixed chunk"))
            });
            ensure!(a.iter().all(|x| x.is_finite()), "nonfinite PHYS geometry");
            Ok(a)
        })
        .collect()
}
fn vertex_bounds(v: &[[f32; 3]]) -> Result<[[f32; 3]; 2]> {
    ensure!(!v.is_empty(), "empty PHYS vertices");
    Ok([
        std::array::from_fn(|i| v.iter().map(|v| v[i]).fold(f32::INFINITY, f32::min)),
        std::array::from_fn(|i| v.iter().map(|v| v[i]).fold(f32::NEG_INFINITY, f32::max)),
    ])
}
impl Asset {
    pub fn from_kv3(document: &BinaryDocument) -> Result<Self> {
        let value = &document.value;
        let attributes = list(value, "m_collisionAttributes")?.to_vec();
        let surface_hashes = list(value, "m_surfacePropertyHashes")?
            .iter()
            .map(|value| {
                Ok(u32::try_from(
                    value.as_u64().context("invalid PHYS surface hash")?,
                )?)
            })
            .collect::<Result<Vec<_>>>()?;
        let mut shapes = Vec::new();
        let mut bounds = Vec::new();
        for (part, p) in list(value, "m_parts")?.iter().enumerate() {
            let body = &p["m_rnShape"];
            ensure!(
                list(body, "m_spheres")?.is_empty() && list(body, "m_capsules")?.is_empty(),
                "unsupported static PHYS primitive"
            );
            for (is_hull, key) in [(true, "m_hulls"), (false, "m_meshes")] {
                for (si, s) in list(body, key)?.iter().enumerate() {
                    let attribute = index(s, "m_nCollisionAttributeIndex")?;
                    ensure!(attribute < attributes.len(), "PHYS attribute index");
                    let surface = index(s, "m_nSurfacePropertyIndex")?;
                    ensure!(surface < surface_hashes.len(), "PHYS surface index");
                    let (geometry, bound) = if is_hull {
                        let g = &s["m_Hull"];
                        let planes = floats::<4>(document.binary(&g["m_Planes"])?)?;
                        ensure!(!planes.is_empty(), "empty PHYS hull");
                        let vertices = floats::<3>(document.binary(&g["m_VertexPositions"])?)?;
                        let b = vertex_bounds(&vertices)?;
                        for (side, key) in ["m_vMinBounds", "m_vMaxBounds"].iter().enumerate() {
                            let scalar = list(&g["m_Bounds"], key)?;
                            ensure!(scalar.len() == 3, "invalid hull scalar bounds");
                            for axis in 0..3 {
                                let value =
                                    scalar[axis].as_f64().context("invalid hull scalar bound")?;
                                ensure!(
                                    (value - f64::from(b[side][axis])).abs() <= 0.00000051,
                                    "PHYS hull bounds differ from binary vertices"
                                );
                            }
                        }

                        // Recover exact scalar bounds from binary positions; the text exporter rounds scalars.
                        let c = std::array::from_fn::<_, 3, _>(|i| (b[0][i] + b[1][i]) * 0.5);
                        let e = std::array::from_fn::<_, 3, _>(|i| (b[1][i] - b[0][i]) * 0.5);
                        (
                            Geometry::Hull(planes),
                            [
                                std::array::from_fn(|i| (c[i] - e[i]) - 0.0625),
                                std::array::from_fn(|i| (c[i] + e[i]) + 0.0625),
                            ],
                        )
                    } else {
                        let g = &s["m_Mesh"];
                        let vertices = floats::<3>(document.binary(&g["m_Vertices"])?)?;
                        let ids = document.binary(&g["m_Triangles"])?;
                        ensure!(ids.len() % 12 == 0, "PHYS triangle stride");
                        let mut triangles = Vec::with_capacity(ids.len() / 12);
                        for chunk in ids.chunks_exact(12) {
                            let mut tri = [[0.; 3]; 3];
                            for i in 0..3 {
                                let vi = u32::from_le_bytes(
                                    chunk[i * 4..i * 4 + 4].try_into().expect("fixed chunk"),
                                ) as usize;
                                tri[i] = *vertices.get(vi).context("PHYS triangle vertex index")?;
                            }
                            triangles.push(tri);
                        }
                        let raw = document.binary(&g["m_Nodes"])?;
                        ensure!(
                            !raw.is_empty() && raw.len() % 32 == 0,
                            "PHYS mesh tree stride"
                        );
                        let mut nodes = Vec::with_capacity(raw.len() / 32);
                        for b in raw.chunks_exact(32) {
                            let bounds = [floats::<3>(&b[..12])?[0], floats::<3>(&b[16..28])?[0]];
                            ensure!(
                                (0..3).all(|i| bounds[0][i] <= bounds[1][i]),
                                "invalid mesh bounds"
                            );
                            nodes.push(mesh_bvh::Node {
                                bounds,
                                descriptor: u32::from_le_bytes(
                                    b[12..16].try_into().expect("fixed chunk"),
                                ),
                                first: u32::from_le_bytes(
                                    b[28..32].try_into().expect("fixed chunk"),
                                ),
                            });
                        }
                        let mut seen = vec![false; nodes.len()];
                        let mut stack = vec![0usize];
                        while let Some(ni) = stack.pop() {
                            ensure!(!seen[ni], "shared or cyclic PHYS mesh tree");
                            seen[ni] = true;
                            let n = &nodes[ni];
                            let offset = (n.descriptor & 0x3fff_ffff) as usize;
                            if n.descriptor >> 30 == 3 {
                                let end = (n.first as usize)
                                    .checked_add(offset)
                                    .context("PHYS triangle range overflow")?;
                                ensure!(end <= triangles.len(), "PHYS mesh leaf range");
                            } else {
                                let far = ni.checked_add(offset).context("PHYS child overflow")?;
                                ensure!(
                                    offset > 1 && far < nodes.len() && ni + 1 < nodes.len(),
                                    "PHYS mesh child range"
                                );
                                stack.push(far);
                                stack.push(ni + 1);
                            }
                        }
                        let b = nodes[0].bounds;
                        let flags = u32::try_from(index(g, "m_nFlags")?)?;
                        ensure!(flags <= 3, "unsupported PHYS mesh flags");
                        let materials = list(g, "m_Materials")?
                            .iter()
                            .map(|v| {
                                Ok(u8::try_from(v.as_u64().context("invalid mesh material")?)?)
                            })
                            .collect::<Result<Vec<_>>>()?;
                        ensure!(
                            (materials.is_empty() || materials.len() == triangles.len())
                                && materials
                                    .iter()
                                    .all(|index| usize::from(*index) < surface_hashes.len()),
                            "PHYS material count"
                        );
                        (
                            Geometry::Mesh {
                                triangles,
                                nodes,
                                bounds: b,
                                flags,
                                materials,
                            },
                            [b[0].map(|x| x - 0.0625), b[1].map(|x| x + 0.0625)],
                        )
                    };
                    shapes.push(Shape {
                        part,
                        index: si,
                        attribute,
                        surface,
                        geometry,
                    });
                    bounds.push(tree_builder::Bounds {
                        min: bound[0],
                        max: bound[1],
                    });
                }
            }
        }
        let tree = tree_builder::Tree::build(&bounds).map_err(anyhow::Error::msg)?;
        let root = tree.root.map_or(-1, |i| i as i32);
        let nodes = tree
            .nodes
            .iter()
            .map(|n| world_bvh::Node {
                bounds: [n.bounds.min, n.bounds.max],
                left: n.left,
                right: n.right,
                leaf: n.leaf.unwrap_or(0) as u64,
            })
            .collect();
        Ok(Self {
            attributes,
            surface_hashes,
            shapes,
            nodes,
            root,
        })
    }
    /// Delta is the original, unnormalized weapon direction multiplied by its range.
    /// A negative extension collects the complete ray; a finite extension shortens subsequent visits.
    pub fn collect(
        &self,
        start: [f32; 3],
        delta: [f32; 3],
        extension: f32,
        eligible: &[bool],
    ) -> Result<Vec<Hit>> {
        ensure!(
            eligible.len() == self.attributes.len(),
            "collision filter count mismatch"
        );
        ensure!(
            start.iter().chain(delta.iter()).all(|v| v.is_finite()) && extension.is_finite(),
            "invalid collision query"
        );
        let length = ((delta[2] * delta[2] + delta[1] * delta[1]) + delta[0] * delta[0]).sqrt();
        let allowance = if extension < 0. || length <= 0. {
            1.
        } else {
            extension / length
        };
        let mut out = Vec::new();
        world_bvh::walk(
            self.root,
            &self.nodes,
            start,
            delta,
            1.,
            allowance,
            |index, maximum| {
                let shape = &self.shapes[index as usize];
                if !eligible[shape.attribute] {
                    return Ok(1.);
                }
                let hits =
                    self.shape_contacts(index as usize, start, delta, 1., maximum, allowance)?;
                let first = hits.first().map_or(1., |h| h.contact.fraction);
                out.extend(hits);
                Ok(first)
            },
        )?;
        sort_hits(&mut out)?;
        Ok(out)
    }
}

impl Asset {
    pub fn nearest(
        &self,
        start: [f32; 3],
        end: [f32; 3],
        transform: RigidTransform,
        entity: u32,
        eligible: &[bool],
    ) -> Result<Option<NearestHit>> {
        ensure!(
            eligible.len() == self.attributes.len(),
            "nearest filter count mismatch"
        );
        ensure!(start != end, "zero length nearest query");
        let (a, delta) = transform.local_ray(start, std::array::from_fn(|i| end[i] - start[i]))?;
        let reciprocal = 1. / transform.scale;
        let mut best: Option<NearestHit> = None;
        // Hull uses reciprocal multiplication; mesh uses scalar division in the native shape interface.
        let hull_a = a.map(|x| x * reciprocal);
        let hull_delta = delta.map(|x| x * reciprocal);
        let mesh_a = a.map(|x| x / transform.scale);
        let mesh_delta = delta.map(|x| x / transform.scale);
        let hull_box = query_bounds(hull_a, hull_delta);
        let mesh_box = query_bounds(mesh_a, mesh_delta);
        let query_box = [
            std::array::from_fn(|i| hull_box[0][i].min(mesh_box[0][i])),
            std::array::from_fn(|i| hull_box[1][i].max(mesh_box[1][i])),
        ];
        ensure!(
            query_box.iter().flatten().all(|v| v.is_finite()),
            "nearest ray overflow"
        );
        let mut candidates = Vec::new();
        let mut stack = Vec::new();
        if self.root >= 0 {
            stack.push(self.root as usize);
        }
        while let Some(i) = stack.pop() {
            let n = &self.nodes[i];
            if !bounds_overlap(n.bounds, query_box) {
                continue;
            }
            if n.left < 0 {
                candidates.push(n.leaf as usize);
            } else {
                stack.push(n.right as usize);
                stack.push(n.left as usize);
            }
        }
        // Keep resource order for identical nearest fractions, independently of tree visitation.
        candidates.sort_unstable();
        for i in candidates {
            let s = &self.shapes[i];
            if !eligible[s.attribute] {
                continue;
            }
            let result = match &s.geometry {
                Geometry::Hull(planes) => nearest::hull_contact(planes, hull_a, hull_delta),
                Geometry::Mesh {
                    triangles, nodes, ..
                } => {
                    let mut ids = Vec::new();
                    let mut stack = vec![0usize];
                    while let Some(i) = stack.pop() {
                        let n = &nodes[i];
                        if !bounds_overlap(n.bounds, mesh_box) {
                            continue;
                        }
                        let offset = (n.descriptor & 0x3fff_ffff) as usize;
                        if n.descriptor >> 30 == 3 {
                            ids.extend(n.first as usize..n.first as usize + offset);
                        } else {
                            stack.push(i + offset);
                            stack.push(i + 1);
                        }
                    }
                    ids.sort_unstable();
                    ids.dedup();
                    ids.into_iter()
                        .filter_map(|i| {
                            nearest::triangle_fraction_delta(triangles[i], mesh_a, mesh_delta)
                        })
                        .min_by(f32::total_cmp)
                        .map(|f| (f, false))
                }
            };
            if let Some((fraction, start_solid)) = result {
                if best.is_none_or(|old| fraction < old.fraction) {
                    best = Some(NearestHit {
                        fraction,
                        start_solid,
                        entity,
                        part: s.part,
                        shape: s.index,
                        attribute: s.attribute,
                        surface: s.surface,
                    });
                }
            }
        }
        Ok(best)
    }
}

impl Asset {
    pub fn shape_count(&self) -> usize {
        self.shapes.len()
    }
    pub fn shape_attribute(&self, index: usize) -> Option<usize> {
        self.shapes.get(index).map(|shape| shape.attribute)
    }
    /// Collect one rigid shape in local-normal convention. The caller owns scene order and filtering.
    pub fn collect_shape_transformed(
        &self,
        index: usize,
        start: [f32; 3],
        delta: [f32; 3],
        transform: RigidTransform,
        maximum: f32,
        allowance: f32,
    ) -> Result<Vec<Hit>> {
        ensure!(
            maximum.is_finite() && maximum >= 0. && allowance.is_finite() && allowance >= 0.,
            "invalid shape range"
        );
        let (a, d) = transform.local_ray(start, delta)?;
        self.shape_contacts(index, a, d, transform.scale, maximum, allowance)
    }
    fn shape_contacts(
        &self,
        index: usize,
        start: [f32; 3],
        delta: [f32; 3],
        scale: f32,
        maximum: f32,
        allowance: f32,
    ) -> Result<Vec<Hit>> {
        let shape = self.shapes.get(index).context("shape index out of range")?;
        let mut out = Vec::new();
        let mut add = |contact, triangle, surface| {
            out.push(Hit {
                contact,
                part: shape.part,
                shape: shape.index,
                attribute: shape.attribute,
                triangle,
                surface,
            })
        };
        match &shape.geometry {
            Geometry::Hull(planes) => {
                for c in hull(planes, start, delta, scale, maximum)? {
                    add(c, None, shape.surface);
                }
            }
            Geometry::Mesh {
                triangles,
                nodes,
                bounds,
                flags,
                materials,
            } => {
                let found = mesh_bvh::collect(
                    nodes,
                    triangles,
                    start.map(|x| x / scale),
                    delta.map(|x| x / scale),
                    maximum,
                    allowance,
                )?;
                if !found.is_empty()
                    && flags & 1 != 0
                    && mesh_inside(triangles, *bounds, start, [scale; 3], *flags)
                {
                    let inv = 1.
                        / ((delta[1] * delta[1] + delta[2] * delta[2]) + delta[0] * delta[0])
                            .sqrt();
                    add(
                        Contact {
                            fraction: 0.,
                            normal: delta.map(|x| -(x * inv)),
                            exit: false,
                        },
                        None,
                        shape.surface,
                    );
                }
                for (i, fraction, exit) in found {
                    add(
                        Contact {
                            fraction,
                            normal: triangle_normal(triangles[i]),
                            exit,
                        },
                        Some(i),
                        if materials.is_empty() {
                            shape.surface
                        } else {
                            materials[i] as usize
                        },
                    );
                }
            }
        }
        Ok(out)
    }
}

/// Order aggregated contacts without inventing identity order for large equal-key groups.
pub fn sort_hits(hits: &mut [Hit]) -> Result<()> {
    ensure!(
        hits.iter().all(|h| h.contact.fraction.is_finite()),
        "nonfinite collision fraction"
    );
    hits.sort_by(|a, b| {
        a.contact
            .fraction
            .partial_cmp(&b.contact.fraction)
            .expect("finite fractions")
            .then(a.contact.exit.cmp(&b.contact.exit))
    });
    ensure!(
        hits.len() <= 32
            || !hits
                .windows(2)
                .any(|w| w[0].contact.fraction == w[1].contact.fraction
                    && w[0].contact.exit == w[1].contact.exit),
        "unqualified equal-key collision ordering above 32 hits"
    );
    Ok(())
}

fn query_bounds(start: [f32; 3], delta: [f32; 3]) -> [[f32; 3]; 2] {
    let end = std::array::from_fn::<_, 3, _>(|i| start[i] + delta[i]);
    [
        std::array::from_fn(|i| start[i].min(end[i])),
        std::array::from_fn(|i| start[i].max(end[i])),
    ]
}
fn bounds_overlap(a: [[f32; 3]; 2], b: [[f32; 3]; 2]) -> bool {
    (0..3).all(|i| a[0][i] <= b[1][i] && b[0][i] <= a[1][i])
}
#[cfg(test)]
mod tests {
    use super::*;
    fn box_asset() -> Asset {
        let planes = vec![
            [1., 0., 0., 10.],
            [-1., 0., 0., 10.],
            [0., 1., 0., 10.],
            [0., -1., 0., 10.],
            [0., 0., 1., 10.],
            [0., 0., -1., 10.],
        ];
        Asset {
            attributes: vec![Value::Null],
            surface_hashes: vec![1],
            shapes: vec![Shape {
                part: 0,
                index: 0,
                attribute: 0,
                surface: 0,
                geometry: Geometry::Hull(planes),
            }],
            nodes: vec![world_bvh::Node {
                bounds: [[-10.0625; 3], [10.0625; 3]],
                left: -1,
                right: -1,
                leaf: 0,
            }],
            root: 0,
        }
    }
    #[test]
    fn nearest_keeps_negative_entry_separate_from_start_solid() {
        let asset = box_asset();
        let near = asset
            .nearest(
                [10.01, 0., 0.],
                [9., 0., 0.],
                RigidTransform::IDENTITY,
                7,
                &[true],
            )
            .unwrap()
            .unwrap();
        assert!(near.fraction < 0.);
        assert!(!near.start_solid);
        let inside = asset
            .nearest([0.; 3], [1., 0., 0.], RigidTransform::IDENTITY, 7, &[true])
            .unwrap()
            .unwrap();
        assert_eq!(inside.fraction, 0.);
        assert!(inside.start_solid);
        assert!(asset
            .nearest(
                [11., 0., 0.],
                [12., 0., 0.],
                RigidTransform::IDENTITY,
                7,
                &[true]
            )
            .unwrap()
            .is_none());
    }
    #[test]
    fn large_equal_key_lists_are_not_given_invented_identity_order() {
        let mut hits = (0..33)
            .map(|i| Hit {
                contact: Contact {
                    fraction: 0.5,
                    normal: [1., 0., 0.],
                    exit: false,
                },
                part: 0,
                shape: i,
                attribute: 0,
                triangle: None,
                surface: 0,
            })
            .collect::<Vec<_>>();
        assert!(sort_hits(&mut hits).is_err());
        for (i, h) in hits.iter_mut().enumerate() {
            h.contact.fraction = i as f32 / 33.;
        }
        assert!(sort_hits(&mut hits).is_ok());
    }
}
