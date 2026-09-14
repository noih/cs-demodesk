//! Source-bound static obstruction cache shared by the entire match.
use super::line_of_sight::{Triangle, World};
use anyhow::{ensure, Context, Result};
use serde_json::Value;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
    process::Command,
};
fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = sha1_smol::Sha1::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(hash.digest().to_string())
}
pub struct Prepared {
    pub world: World,
    pub fingerprint: String,
    pub bytes: u64,
}
pub fn prepare(root: &Path, game: &Path, vrf: &Path, map: &str) -> Result<Prepared> {
    ensure!(
        !map.is_empty() && map.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "unsupported map resource name"
    );
    let package = game.join("game/csgo/maps").join(format!("{map}.vpk"));
    let key = format!("{}:{}:los-2", digest(&package)?, digest(vrf)?);
    let dir = root
        .join("analysis/visibility")
        .join(sha1_smol::Sha1::from(key).digest().to_string());
    fs::create_dir_all(&dir)?;
    let destination = dir.join("world_physics_physics.gltf");
    if !dir.join("ready").is_file() {
        let staging = tempfile::tempdir_in(&dir)?;
        let output = crate::render::process::ProcessTree::new()?.output(
            Command::new(vrf)
                .arg("-i")
                .arg(&package)
                .arg("-f")
                .arg(format!("maps/{map}/world_physics.vmdl_c"))
                .arg("-o")
                .arg(staging.path().join("world_physics.gltf"))
                .arg("-d")
                .arg("--gltf_export_format")
                .arg("gltf"),
        )?;
        ensure!(
            output.status.success(),
            "map obstruction export failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let file = staging.path().join("world_physics_physics.gltf");
        let (_, buffer) = load(&file)?;
        let fingerprint = format!("{}:{}", digest(&file)?, digest(&buffer)?);
        for entry in fs::read_dir(staging.path())? {
            let entry = entry?;
            if entry.file_type()?.is_file() {
                fs::copy(entry.path(), dir.join(entry.file_name()))?;
            }
        }
        fs::write(dir.join("ready"), fingerprint)?;
    }
    let (world, buffer) = load(&destination)?;
    let fingerprint = format!("{}:{}", digest(&destination)?, digest(&buffer)?);
    ensure!(
        fs::read_to_string(dir.join("ready"))? == fingerprint,
        "map obstruction cache changed"
    );
    Ok(Prepared {
        world,
        fingerprint,
        bytes: fs::metadata(&destination)?.len() + fs::metadata(buffer)?.len(),
    })
}
/// Lossless blob storage for native collision preparation, independent of the LOS export.
pub struct CollisionSource {
    pub document: super::kv3_text::BinaryDocument,
    pub fingerprint: String,
    pub bytes: u64,
}
pub fn collision_source(
    root: &Path,
    game: &Path,
    vrf: &Path,
    map: &str,
) -> Result<CollisionSource> {
    ensure!(
        !map.is_empty() && map.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'),
        "unsupported map resource name"
    );
    let package = game.join("game/csgo/maps").join(format!("{map}.vpk"));
    let source = format!("{}:{}:collision-1", digest(&package)?, digest(vrf)?);
    let directory = root
        .join("analysis/collision")
        .join(sha1_smol::Sha1::from(&source).digest().to_string());
    fs::create_dir_all(&directory)?;
    let data = directory.join("world.kv3.gz");
    let ready = directory.join("ready");
    let fresh = if !ready.is_file() {
        let staging = tempfile::tempdir_in(&directory)?;
        let raw = staging.path().join("world.vmdl_c");
        let extract = crate::render::process::ProcessTree::new()?.output(
            Command::new(vrf)
                .arg("-i")
                .arg(&package)
                .arg("-f")
                .arg(format!("maps/{map}/world_physics.vmdl_c"))
                .arg("-o")
                .arg(&raw),
        )?;
        ensure!(
            extract.status.success() && raw.is_file(),
            "collision resource extraction failed: {}",
            String::from_utf8_lossy(&extract.stderr)
        );
        let decoded = crate::render::process::ProcessTree::new()?
            .output(Command::new(vrf).arg("-i").arg(&raw).arg("-b").arg("PHYS"))?;
        ensure!(
            decoded.status.success(),
            "collision resource decoding failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        let text = String::from_utf8(decoded.stdout).context("collision resource is not UTF-8")?;
        let document = super::kv3_text::parse_binary(&text)?;
        ensure!(
            document.value["m_parts"].is_array(),
            "missing collision parts"
        );
        let temporary = staging.path().join("world.kv3.gz");
        let mut compressed = flate2::write::GzEncoder::new(
            fs::File::create(&temporary)?,
            flate2::Compression::default(),
        );
        compressed.write_all(text.as_bytes())?;
        compressed.finish()?.sync_all()?;
        let hash = digest(&temporary)?;
        fs::copy(&temporary, &data)?;
        fs::write(&ready, hash)?;
        Some(document)
    } else {
        None
    };
    let hash = digest(&data)?;
    ensure!(
        fs::read_to_string(&ready)? == hash,
        "collision cache changed"
    );
    let document = match fresh {
        Some(document) => document,
        None => {
            let mut text = String::new();
            flate2::read::GzDecoder::new(std::io::BufReader::new(fs::File::open(&data)?))
                .take(256 * 1024 * 1024 + 1)
                .read_to_string(&mut text)?;
            super::kv3_text::parse_binary(&text)?
        }
    };
    ensure!(
        document.value["m_parts"].is_array(),
        "missing collision parts"
    );
    Ok(CollisionSource {
        document,
        fingerprint: format!("{source}:{hash}"),
        bytes: fs::metadata(data)?.len() + fs::metadata(ready)?.len(),
    })
}
fn number(v: &Value) -> Result<usize> {
    usize::try_from(v.as_u64().context("invalid geometry integer")?)
        .context("oversized geometry integer")
}
struct Accessor<'a> {
    bytes: &'a [u8],
    count: usize,
    stride: usize,
    width: usize,
    size: usize,
    kind: u64,
}
impl Accessor<'_> {
    fn value(&self, index: usize, component: usize) -> Result<f64> {
        ensure!(
            index < self.count && component < self.width,
            "invalid geometry index"
        );
        let offset = index * self.stride + component * self.size;
        let b = self
            .bytes
            .get(offset..offset + self.size)
            .context("truncated geometry")?;
        Ok(match self.kind {
            5126 => f64::from(f32::from_le_bytes(b.try_into()?)),
            5125 => f64::from(u32::from_le_bytes(b.try_into()?)),
            5123 => f64::from(u16::from_le_bytes(b.try_into()?)),
            5121 => f64::from(b[0]),
            _ => unreachable!(),
        })
    }
    fn vertex(&self, index: usize) -> Result<[f64; 3]> {
        Ok([
            self.value(index, 0)?,
            self.value(index, 1)?,
            self.value(index, 2)?,
        ])
    }
}
fn accessor<'a>(g: &Value, bytes: &'a [u8], index: &Value, vector: bool) -> Result<Accessor<'a>> {
    let a = &g["accessors"][number(index)?];
    ensure!(
        a["sparse"].is_null() && !a["normalized"].as_bool().unwrap_or(false),
        "unsupported geometry accessor"
    );
    let view = &g["bufferViews"][number(&a["bufferView"])?];
    ensure!(
        view["buffer"].as_u64() == Some(0),
        "unexpected geometry buffer"
    );
    let kind = a["componentType"]
        .as_u64()
        .context("missing geometry component")?;
    let size = match kind {
        5126 | 5125 => 4,
        5123 => 2,
        5121 => 1,
        _ => anyhow::bail!("unsupported geometry component"),
    };
    let width = if vector { 3 } else { 1 };
    ensure!(
        a["type"].as_str() == Some(if vector { "VEC3" } else { "SCALAR" })
            && (!vector || kind == 5126)
            && (vector || kind != 5126),
        "unexpected geometry accessor type"
    );
    let base = number(&view["byteOffset"].as_u64().unwrap_or(0).into())?;
    let length = number(&view["byteLength"])?;
    let view_bytes = bytes
        .get(base..base.checked_add(length).context("geometry view overflow")?)
        .context("geometry view exceeds buffer")?;
    let offset = number(&a["byteOffset"].as_u64().unwrap_or(0).into())?;
    let bytes = view_bytes
        .get(offset..)
        .context("geometry accessor exceeds view")?;
    let stride = number(
        &view["byteStride"]
            .as_u64()
            .unwrap_or((size * width) as u64)
            .into(),
    )?;
    let count = number(&a["count"])?;
    ensure!(stride >= size * width, "invalid geometry stride");
    let end = if count == 0 {
        0
    } else {
        (count - 1)
            .checked_mul(stride)
            .and_then(|n| n.checked_add(size * width))
            .context("geometry accessor overflow")?
    };
    ensure!(end <= bytes.len(), "truncated geometry accessor");
    Ok(Accessor {
        bytes,
        count,
        stride,
        width,
        size,
        kind,
    })
}
pub fn load(file: &Path) -> Result<(World, std::path::PathBuf)> {
    let g: Value = serde_json::from_slice(&fs::read(file)?)?;
    ensure!(
        g["asset"]["generator"]
            .as_str()
            .is_some_and(|s| s.starts_with("Source 2 Viewer ")),
        "unexpected geometry exporter"
    );
    ensure!(
        g["buffers"].as_array().is_some_and(|a| a.len() == 1),
        "unexpected geometry buffers"
    );
    let uri = g["buffers"][0]["uri"]
        .as_str()
        .context("missing geometry buffer")?;
    ensure!(
        !uri.is_empty() && uri != ".." && !uri.contains(['/', '\\', ':']),
        "geometry buffer must be a sibling file"
    );
    let buffer = file
        .parent()
        .context("missing geometry directory")?
        .join(uri);
    let bytes = fs::read(&buffer)?;
    let nodes = g["nodes"].as_array().context("missing geometry nodes")?;
    let mut triangles = vec![];
    let matrix = &nodes.first().context("empty geometry nodes")?["matrix"];
    ensure!(
        matrix.as_array().is_some_and(
            |m| m.len() == 16 && m.iter().all(|n| n.as_f64().is_some_and(f64::is_finite))
        ),
        "missing geometry coordinate conversion"
    );
    ensure!(
        g["buffers"][0]["byteLength"].as_u64() == Some(bytes.len() as u64),
        "geometry buffer length differs"
    );
    // Source 2 Viewer's export-only Source-units to glTF root transform.
    let expected = [
        0., 0., 0.0254, 0., 0.0254, 0., 0., 0., 0., 0.0254, 0., 0., 0., 0., 0., 1.,
    ];
    ensure!(
        matrix
            .as_array()
            .unwrap()
            .iter()
            .zip(expected)
            .all(|(actual, expected)| (actual.as_f64().unwrap() - expected).abs() < 1e-7),
        "unsupported geometry coordinate conversion"
    );
    for node in nodes {
        ensure!(
            &node["matrix"] == matrix
                && node["children"].is_null()
                && node["translation"].is_null()
                && node["rotation"].is_null()
                && node["scale"].is_null(),
            "unsupported geometry transform"
        );
        let surface = node["extras"]["SurfaceProperty"]
            .as_str()
            .context("missing geometry surface")?;
        let layers = node["extras"]["InteractAs"]
            .as_array()
            .context("missing geometry interaction layers")?;
        if layers.iter().any(|v| {
            matches!(
                v.as_str(),
                Some("npcclip" | "playerclip" | "csgo_grenadeclip")
            )
        }) {
            continue;
        }
        // ponytail: physics surfaces approximate visual occlusion; unknown materials stay unqualified.
        let opaque = !surface.contains("glass")
            && !surface.contains("chainlink")
            && !surface.starts_with("vrf_unknown")
            && surface != "default";
        let mesh = &g["meshes"][number(&node["mesh"])?];
        for primitive in mesh["primitives"]
            .as_array()
            .context("missing geometry primitives")?
        {
            ensure!(
                primitive["mode"].as_u64().unwrap_or(4) == 4,
                "unsupported geometry primitive"
            );
            let vertices = accessor(&g, &bytes, &primitive["attributes"]["POSITION"], true)?;
            let indices = accessor(&g, &bytes, &primitive["indices"], false)?;
            ensure!(indices.count % 3 == 0, "incomplete geometry triangles");
            for i in (0..indices.count).step_by(3) {
                let vertex = |j| vertices.vertex(indices.value(j, 0)? as usize);
                // Physics POSITION values remain Source game units; the shared root is export-only.
                triangles.push(Triangle {
                    vertices: [vertex(i)?, vertex(i + 1)?, vertex(i + 2)?],
                    opaque,
                });
            }
        }
    }
    Ok((World::new(triangles)?, buffer))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn physics_loader_validates_buffer_and_transform() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("mesh.gltf");
        let mut bytes = vec![];
        for value in [5f32, -10., -10., 5., 10., -10., 5., 0., 10.] {
            bytes.extend(value.to_le_bytes());
        }
        for value in [0u16, 1, 2] {
            bytes.extend(value.to_le_bytes());
        }
        fs::write(dir.path().join("mesh.bin"), &bytes).unwrap();
        let mut data = serde_json::json!({"asset":{"generator":"Source 2 Viewer test"},"buffers":[{"uri":"mesh.bin","byteLength":42}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"indices":1}]}],"nodes":[{"matrix":[0.,0.,0.0254,0.,0.0254,0.,0.,0.,0.,0.0254,0.,0.,0.,0.,0.,1.],"mesh":0,"extras":{"SurfaceProperty":"concrete","InteractAs":[]}}]});
        fs::write(&file, serde_json::to_vec(&data).unwrap()).unwrap();
        let (world, _) = load(&file).unwrap();
        assert_eq!(
            world.ray([0., 0., 0.], [10., 0., 0.]),
            super::super::line_of_sight::Occlusion::Blocked
        );
        data["nodes"][0]["matrix"][12] = 1.into();
        fs::write(&file, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(load(&file).is_err());
        data["nodes"][0]["matrix"][12] = 0.into();
        data["buffers"][0]["uri"] = "../outside.bin".into();
        fs::write(&file, serde_json::to_vec(&data).unwrap()).unwrap();
        assert!(load(&file).is_err());
        data["buffers"][0]["uri"] = "mesh.bin".into();
        fs::write(&file, serde_json::to_vec(&data).unwrap()).unwrap();
        fs::write(dir.path().join("mesh.bin"), &bytes[..41]).unwrap();
        assert!(load(&file).is_err());
    }
}
