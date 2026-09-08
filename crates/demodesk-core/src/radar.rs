//! Radar images and world→image mapping for the 2D replay, pulled out of the
//! game's own files with Source2Viewer-CLI on first use:
//!   radar/<map>/overview.txt    resource/overviews/<map>.txt from pak01_dir.vpk (KeyValues)
//!   radar/<map>/<layer>.png     panorama/images/overheadmaps/<map>[_<layer>]_radar_psd.vtex_c
//!   radar/<map>/overview.json   [`MapAssets`] — the parsed numbers + which png is which layer
//! Everything is disposable; delete the folder and it is rebuilt. overview.json
//! carries the layout version and the game's PatchVersion it was extracted
//! from, so a map rework or a layout change re-extracts automatically.

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapLayer {
    /// "default", "lower", … as named in `verticalsections`
    pub name: String,
    /// png file name inside the map folder
    pub image: String,
    /// absolute path of that png, filled in when the assets are read (not stored)
    #[serde(skip_deserializing, default)]
    pub path: PathBuf,
    pub altitude_min: f64,
    pub altitude_max: f64,
}

/// Bump when the fields / files below change; older folders are re-extracted.
pub const RADAR_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct MapAssets {
    #[serde(default)]
    pub schema_version: u32,
    /// CS2 PatchVersion (steam.inf) the files were extracted from
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch_version: Option<u32>,
    pub map_name: String,
    /// folder holding overview.json and the pngs
    pub dir: PathBuf,
    /// world x/y of the image's top-left corner
    pub pos_x: f64,
    pub pos_y: f64,
    /// world units per image pixel (the images are 1024×1024)
    pub scale: f64,
    /// ordered top layer first; always at least one entry
    pub layers: Vec<MapLayer>,
}

// ---- KeyValues (VDF) ----

#[derive(Debug, Clone, PartialEq)]
pub enum Kv {
    Str(String),
    Obj(Vec<(String, Kv)>),
}

impl Kv {
    pub fn get(&self, key: &str) -> Option<&Kv> {
        match self {
            Kv::Obj(items) => items.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v),
            _ => None,
        }
    }
    pub fn str(&self, key: &str) -> Option<&str> {
        match self.get(key)? {
            Kv::Str(s) => Some(s.as_str()),
            _ => None,
        }
    }
    pub fn num(&self, key: &str) -> Option<f64> {
        self.str(key)?.trim().parse().ok()
    }
    pub fn entries(&self) -> &[(String, Kv)] {
        match self {
            Kv::Obj(items) => items,
            _ => &[],
        }
    }
}

/// Minimal Valve KeyValues parser: quoted or bare tokens, `{ }` nesting, `//` comments.
pub fn parse_kv(text: &str) -> Result<Kv> {
    let mut tokens: Vec<String> = vec![];
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '"' => {
                let mut s = String::new();
                for ch in chars.by_ref() {
                    if ch == '"' {
                        break;
                    }
                    s.push(ch);
                }
                tokens.push(s);
            }
            '{' | '}' => tokens.push(c.to_string()),
            '/' if chars.peek() == Some(&'/') => {
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        break;
                    }
                }
            }
            c if c.is_whitespace() => {}
            c => {
                let mut s = String::from(c);
                while let Some(&n) = chars.peek() {
                    if n.is_whitespace() || n == '{' || n == '}' || n == '"' {
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                tokens.push(s);
            }
        }
    }
    let mut pos = 0;
    let root = parse_obj(&tokens, &mut pos)?;
    Ok(root)
}

fn parse_obj(tokens: &[String], pos: &mut usize) -> Result<Kv> {
    let mut items = vec![];
    while *pos < tokens.len() {
        let key = &tokens[*pos];
        if key == "}" {
            *pos += 1;
            return Ok(Kv::Obj(items));
        }
        *pos += 1;
        let val = tokens.get(*pos).ok_or_else(|| anyhow!("KeyValues: value missing after {key}"))?;
        *pos += 1;
        if val == "{" {
            items.push((key.clone(), parse_obj(tokens, pos)?));
        } else {
            items.push((key.clone(), Kv::Str(val.clone())));
        }
    }
    Ok(Kv::Obj(items))
}

/// The map's own overview block (the file is `"de_x" { … }`; fall back to the first object).
fn overview_block<'a>(root: &'a Kv, map: &str) -> Option<&'a Kv> {
    root.get(map).or_else(|| root.entries().iter().find(|(_, v)| matches!(v, Kv::Obj(_))).map(|(_, v)| v))
}

/// Numbers + layers from overview.txt. Layers without a matching png are dropped
/// by [`ensure_map_assets`]; here every section is listed with its expected file name.
pub fn parse_overview(text: &str, map: &str) -> Result<(f64, f64, f64, Vec<MapLayer>)> {
    let root = parse_kv(text)?;
    let ov = overview_block(&root, map).ok_or_else(|| anyhow!("overview.txt has no {map} block"))?;
    let pos_x = ov.num("pos_x").ok_or_else(|| anyhow!("pos_x missing"))?;
    let pos_y = ov.num("pos_y").ok_or_else(|| anyhow!("pos_y missing"))?;
    let scale = ov.num("scale").ok_or_else(|| anyhow!("scale missing"))?;
    let mut layers: Vec<MapLayer> = vec![];
    if let Some(sections) = ov.get("verticalsections") {
        for (name, sec) in sections.entries() {
            layers.push(MapLayer {
                name: name.clone(),
                image: if name.eq_ignore_ascii_case("default") { "default.png".into() } else { format!("{}.png", name.to_ascii_lowercase()) },
                path: PathBuf::new(),
                altitude_min: sec.num("AltitudeMin").unwrap_or(-1.0e6),
                altitude_max: sec.num("AltitudeMax").unwrap_or(1.0e6),
            });
        }
    }
    if layers.is_empty() {
        layers.push(MapLayer { name: "default".into(), image: "default.png".into(), path: PathBuf::new(), altitude_min: -1.0e6, altitude_max: 1.0e6 });
    }
    layers.sort_by(|a, b| b.altitude_max.total_cmp(&a.altitude_max));
    Ok((pos_x, pos_y, scale, layers))
}

pub fn pak_path(cs2_dir: &Path) -> PathBuf {
    cs2_dir.join("game").join("csgo").join("pak01_dir.vpk")
}

fn run_vrf(vrf: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new(vrf).args(args).output().with_context(|| format!("running {}", vrf.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr);
        return Err(anyhow!("Source2Viewer-CLI {} failed: {}{}", args.join(" "), stdout.trim(), stderr.trim()));
    }
    Ok(stdout)
}

/// Extract one file from the vpk to `dest` (decompiled: .vtex_c → .png, .txt as is).
fn extract(vrf: &Path, pak: &Path, inner: &str, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest.parent().unwrap())?;
    let _ = fs::remove_file(dest);
    let out = run_vrf(vrf, &["-i", &pak.to_string_lossy(), "-f", inner, "-o", &dest.to_string_lossy(), "-d"])?;
    if !dest.is_file() {
        return Err(anyhow!("{inner} not extracted: {}", out.trim()));
    }
    Ok(())
}

/// Layer name from a radar file name: `de_nuke_radar_psd.vtex_c` → "default",
/// `de_nuke_lower_radar.ctex_c` → "lower"; anything else → None.
fn radar_layer(map: &str, file_name: &str) -> Option<String> {
    let stem = file_name.split('.').next()?.to_ascii_lowercase();
    let stem = stem.strip_suffix("_psd").unwrap_or(&stem);
    let stem = stem.strip_suffix("_radar")?;
    let rest = stem.strip_prefix(&map.to_ascii_lowercase())?;
    if rest.is_empty() {
        Some("default".into())
    } else {
        rest.strip_prefix('_').filter(|r| !r.is_empty() && !r.contains('_')).map(|r| r.to_string())
    }
}

/// Radar image files for `map` inside the vpk, keyed by layer name ("default" for the main one).
fn list_radar_images(vrf: &Path, pak: &Path, map: &str) -> Result<BTreeMap<String, String>> {
    let prefix = format!("panorama/images/overheadmaps/{map}");
    let out = run_vrf(vrf, &["-i", &pak.to_string_lossy(), "--vpk_list", "-f", &prefix])?;
    let mut found = BTreeMap::new();
    for line in out.lines() {
        // the path may be surrounded by other columns; take it from the prefix to the next blank
        let Some(start) = line.find(&prefix) else { continue };
        let path = line[start..].split_whitespace().next().unwrap_or("");
        let file_name = path.rsplit('/').next().unwrap_or(path);
        if let Some(layer) = radar_layer(map, file_name) {
            found.entry(layer).or_insert_with(|| path.to_string());
        }
    }
    Ok(found)
}

pub fn assets_path(radar_dir: &Path, map: &str) -> PathBuf {
    radar_dir.join(map).join("overview.json")
}

/// The stored assets, only if they are complete, current, and (when the game
/// version is known) were extracted from this game version.
pub fn read_map_assets(radar_dir: &Path, map: &str, patch_version: Option<u32>) -> Option<MapAssets> {
    let mut a: MapAssets = serde_json::from_str(&fs::read_to_string(assets_path(radar_dir, map)).ok()?).ok()?;
    a.dir = radar_dir.join(map);
    let current = a.schema_version == RADAR_SCHEMA_VERSION && (patch_version.is_none() || a.patch_version == patch_version);
    for l in &mut a.layers {
        l.path = a.dir.join(&l.image);
    }
    (current && a.layers.iter().all(|l| l.path.is_file())).then_some(a)
}

/// Make sure `radar/<map>/` holds the radar png(s) and overview.json, extracting
/// them with Source2Viewer-CLI when missing or stale.
pub fn ensure_map_assets(radar_dir: &Path, cs2_dir: &Path, vrf: &Path, map: &str, patch_version: Option<u32>) -> Result<MapAssets> {
    if let Some(a) = read_map_assets(radar_dir, map, patch_version) {
        return Ok(a);
    }
    let pak = pak_path(cs2_dir);
    if !pak.is_file() {
        return Err(anyhow!("{} not found", pak.display()));
    }
    let dir = radar_dir.join(map);
    fs::create_dir_all(&dir)?;
    let txt = dir.join("overview.txt");
    extract(vrf, &pak, &format!("resource/overviews/{map}.txt"), &txt)?;
    let (pos_x, pos_y, scale, wanted) = parse_overview(&fs::read_to_string(&txt)?, map)?;
    let images = list_radar_images(vrf, &pak, map)?;
    if images.is_empty() {
        return Err(anyhow!("no radar image for {map} in {}", pak.display()));
    }
    let mut layers = vec![];
    for layer in wanted {
        let Some(inner) = images.get(&layer.name.to_ascii_lowercase()) else { continue };
        extract(vrf, &pak, inner, &dir.join(&layer.image))?;
        layers.push(layer);
    }
    if layers.is_empty() {
        // overview lists no usable sections: take the main image as one layer
        let inner = images.get("default").or_else(|| images.values().next()).unwrap();
        extract(vrf, &pak, inner, &dir.join("default.png"))?;
        layers.push(MapLayer { name: "default".into(), image: "default.png".into(), path: PathBuf::new(), altitude_min: -1.0e6, altitude_max: 1.0e6 });
    }
    for l in &mut layers {
        l.path = dir.join(&l.image);
    }
    let assets = MapAssets { schema_version: RADAR_SCHEMA_VERSION, patch_version, map_name: map.to_string(), dir, pos_x, pos_y, scale, layers };
    let mut stored = assets.clone();
    stored.dir = PathBuf::from(".");
    for layer in &mut stored.layers { layer.path = PathBuf::from(&layer.image); }
    fs::write(assets_path(radar_dir, map), serde_json::to_string_pretty(&stored)?)?;
    Ok(assets)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moved_radar_assets_resolve_from_the_current_directory() {
        let temp = tempfile::tempdir().unwrap();
        let old = temp.path().join("old/radar");
        let map = old.join("de_dust2");
        fs::create_dir_all(&map).unwrap();
        fs::write(map.join("default.png"), b"image").unwrap();
        let assets = MapAssets {
            schema_version: RADAR_SCHEMA_VERSION, patch_version: Some(1),
            map_name: "de_dust2".into(), dir: map.clone(), pos_x: 0.0, pos_y: 0.0, scale: 1.0,
            layers: vec![MapLayer { name: "default".into(), image: "default.png".into(), path: map.join("default.png"), altitude_min: 0.0, altitude_max: 1.0 }],
        };
        fs::write(assets_path(&old, "de_dust2"), serde_json::to_vec(&assets).unwrap()).unwrap();
        let next = temp.path().join("new/radar");
        fs::create_dir_all(next.parent().unwrap()).unwrap();
        fs::rename(&old, &next).unwrap();
        let loaded = read_map_assets(&next, "de_dust2", Some(1)).unwrap();
        assert_eq!(loaded.dir, next.join("de_dust2"));
        assert_eq!(loaded.layers[0].path, next.join("de_dust2/default.png"));
        assert!(read_map_assets(&next, "de_dust2", Some(2)).is_none());
    }

    #[test]
    fn parses_overview_with_sections() {
        let txt = r#"
// comment
"de_nuke"
{
	"material"	"overviews/de_nuke"
	"pos_x"		"-3453"
	"pos_y"		"2887"
	"scale"		"7"
	"rotate"	"0"
	"verticalsections"
	{
		"default"
		{
			"AltitudeMax"	"10000"
			"AltitudeMin"	"-495"
		}
		"lower"
		{
			"AltitudeMax"	"-495"
			"AltitudeMin"	"-10000"
		}
	}
}
"#;
        let (x, y, s, layers) = parse_overview(txt, "de_nuke").unwrap();
        assert_eq!((x, y, s), (-3453.0, 2887.0, 7.0));
        assert_eq!(layers.len(), 2);
        assert_eq!(layers[0].name, "default");
        assert_eq!(layers[1].image, "lower.png");
        assert_eq!(layers[1].altitude_max, -495.0);
    }

    #[test]
    fn radar_layer_names() {
        assert_eq!(radar_layer("de_nuke", "de_nuke_radar_psd.vtex_c").as_deref(), Some("default"));
        assert_eq!(radar_layer("de_nuke", "de_nuke_lower_radar_psd.vtex_c").as_deref(), Some("lower"));
        assert_eq!(radar_layer("de_nuke", "de_nuke_lower_radar.ctex_c").as_deref(), Some("lower"));
        assert_eq!(radar_layer("de_nuke", "de_nuke_radar_spectate.vtex_c"), None);
        assert_eq!(radar_layer("de_nuke", "de_nuke_vanity.vtex_c"), None);
    }

    #[test]
    fn parses_overview_without_sections() {
        let (x, _, s, layers) = parse_overview("\"de_mirage\" { \"pos_x\" \"-3230\" \"pos_y\" \"1713\" \"scale\" \"5\" }", "de_mirage").unwrap();
        assert_eq!((x, s), (-3230.0, 5.0));
        assert_eq!(layers.len(), 1);
    }
}
