//! Shared extraction of recorded clip dependencies from the installed game.
//! Content hashes identify installed assets; they do not assert historical demo compatibility.
use super::{
    animation_clip::Clip,
    kv3_text,
    model_hitboxes::{self, HitboxSet},
};
use anyhow::{ensure, Context, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

const SKELETON: &str = "animation/skeletons/characters/worldmodel.vnmskel";
const FORMAT: u32 = 3;

pub struct Assets {
    pub clips: BTreeMap<String, Clip>,
    pub skeleton: serde_json::Value,
    pub model_skeletons: BTreeMap<u64, Vec<String>>,
    pub model_hitboxes: BTreeMap<u64, Vec<HitboxSet>>,
    pub resource_content_id: String,
    /// All retained compiled resources, DATA text and manifest bytes.
    pub total_bytes: u64,
}

#[derive(Serialize, Deserialize)]
struct FileDigest {
    bytes: u64,
    sha1: String,
}
#[derive(Serialize, Deserialize)]
struct Manifest {
    format: u32,
    source_key: String,
    requested: Vec<String>,
    files: BTreeMap<String, FileDigest>,
    resource_content_id: String,
}

fn validate_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && path.len() <= 1024
            && path.is_ascii()
            && path
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
            && path
                .split('/')
                .all(|part| !part.is_empty() && part != "." && part != ".."),
        "invalid animation resource path"
    );
    Ok(())
}

/// MurmurHash64B of the lowercase, uncompiled resource path.
/// Seed: ValveResourceFormat/ValveResourceFormat README "supported magics";
/// algorithm: aappleby/smhasher src/MurmurHash2.cpp (MurmurHash64B).
pub fn resource_id(path: &str) -> Result<u64> {
    validate_path(path)?;
    let lowercase = path.to_ascii_lowercase();
    let mut bytes = lowercase.as_bytes();
    const M: u32 = 0x5bd1e995;
    fn mix(value: u32) -> u32 {
        let value = value.wrapping_mul(M);
        (value ^ (value >> 24)).wrapping_mul(M)
    }
    let mut h1 = 0xedabcdef ^ bytes.len() as u32;
    let mut h2 = 0u32;
    while bytes.len() >= 8 {
        h1 = h1.wrapping_mul(M) ^ mix(u32::from_le_bytes(bytes[..4].try_into()?));
        h2 = h2.wrapping_mul(M) ^ mix(u32::from_le_bytes(bytes[4..8].try_into()?));
        bytes = &bytes[8..];
    }
    if bytes.len() >= 4 {
        h1 = h1.wrapping_mul(M) ^ mix(u32::from_le_bytes(bytes[..4].try_into()?));
        bytes = &bytes[4..];
    }
    if !bytes.is_empty() {
        for (i, byte) in bytes.iter().enumerate() {
            h2 ^= u32::from(*byte) << (8 * i);
        }
        h2 = h2.wrapping_mul(M);
    }
    h1 = (h1 ^ (h2 >> 18)).wrapping_mul(M);
    h2 = (h2 ^ (h1 >> 22)).wrapping_mul(M);
    h1 = (h1 ^ (h2 >> 17)).wrapping_mul(M);
    h2 = (h2 ^ (h1 >> 19)).wrapping_mul(M);
    Ok(u64::from(h1) << 32 | u64::from(h2))
}

fn hash_file(path: &Path) -> Result<FileDigest> {
    let mut file = fs::File::open(path)?;
    let mut hash = sha1_smol::Sha1::new();
    let mut bytes = 0u64;
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        bytes = bytes
            .checked_add(count as u64)
            .context("asset size overflow")?;
        ensure!(
            bytes <= 1024 * 1024 * 1024,
            "animation resource exceeds size limit"
        );
        hash.update(&buffer[..count]);
    }
    Ok(FileDigest {
        bytes,
        sha1: hash.digest().to_string(),
    })
}

fn run(vrf: &Path, command: &mut Command) -> Result<String> {
    let output = crate::render::process::ProcessTree::new()?
        .output(command)
        .with_context(|| format!("running animation asset decoder {}", vrf.display()))?;
    ensure!(
        output.status.success(),
        "animation asset decoder failed: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    String::from_utf8(output.stdout).context("asset decoder output is not UTF-8")
}

// Broader directory selectors keep the single extraction below Windows' command-line limit.
fn selectors(paths: &[String]) -> Result<String> {
    let mut selected: BTreeSet<String> = paths.iter().map(|p| format!("{p}_c")).collect();
    // VRF treats an exact single-file selector's output as a file, not a directory.
    if selected.len() == 1 {
        let only = selected
            .first()
            .context("missing animation asset selector")?;
        let (parent, _) = only
            .rsplit_once('/')
            .context("animation asset has no directory")?;
        return Ok(format!("{parent}/"));
    }
    while selected.iter().map(|p| p.len() + 1).sum::<usize>() > 20_000 {
        let mut parents = BTreeMap::<String, usize>::new();
        for path in &selected {
            if let Some((parent, _)) = path.trim_end_matches('/').rsplit_once('/') {
                *parents.entry(format!("{parent}/")).or_default() += path.len() + 1;
            }
        }
        let parent = parents
            .into_iter()
            .max_by_key(|(parent, count)| count.saturating_sub(parent.len() + 1))
            .map(|(parent, _)| parent)
            .context("animation asset selectors exceed command limit")?;
        selected.retain(|p| !p.starts_with(&parent));
        selected.insert(parent);
    }
    Ok(selected.into_iter().collect::<Vec<_>>().join(","))
}

fn collect_files(
    base: &Path,
    directory: &Path,
    out: &mut BTreeMap<String, FileDigest>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let kind = entry.file_type()?;
        ensure!(!kind.is_symlink(), "unexpected asset cache symlink");
        if kind.is_dir() {
            collect_files(base, &entry.path(), out)?;
        } else {
            ensure!(kind.is_file(), "unexpected asset cache entry");
            let relative = entry
                .path()
                .strip_prefix(base)?
                .to_string_lossy()
                .replace('\\', "/");
            validate_path(&relative)?;
            out.insert(relative, hash_file(&entry.path())?);
            ensure!(out.len() <= 100_000, "too many animation assets");
        }
    }
    Ok(())
}

fn resource_sections(output: &str) -> Result<Vec<(PathBuf, &str)>> {
    let mut headers = Vec::new();
    let mut offset = 0;
    for line in output.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if let Some(tail) = trimmed.strip_prefix('[') {
            if let Some((progress, path)) = tail.split_once("] ") {
                if let Some((current, total)) = progress.split_once('/') {
                    if current.parse::<u32>().is_ok() && total.parse::<u32>().is_ok() {
                        headers.push((PathBuf::from(path), offset + line.len(), offset));
                    }
                }
            }
        }
        offset += line.len();
    }
    ensure!(
        !headers.is_empty(),
        "asset decoder returned no resource records"
    );
    let mut chunks = Vec::with_capacity(headers.len());
    for (i, (path, begin, _)) in headers.iter().enumerate() {
        let end = headers.get(i + 1).map_or(output.len(), |next| next.2);
        chunks.push((path.clone(), &output[*begin..end]));
    }
    Ok(chunks)
}

fn kv3_block(section: &str) -> Result<&str> {
    let start = section
        .find("<!-- kv3 ")
        .context("resource block is not textual KV3")?;
    let finish = section.rfind("\n}").context("truncated resource block")? + 2;
    ensure!(start < finish, "invalid resource block");
    Ok(&section[start..finish])
}

fn data_chunks(output: &str) -> Result<Vec<(PathBuf, &str)>> {
    resource_sections(output)?
        .into_iter()
        .map(|(path, section)| Ok((path, kv3_block(section)?)))
        .collect()
}

fn mesh_blocks(section: &str) -> Result<Vec<serde_json::Value>> {
    section
        .split("--- Data for block \"MDAT\" ---")
        .skip(1)
        .map(|block| kv3_text::parse(kv3_block(block)?))
        .collect()
}

fn load(cache: &Path, source_key: &str, requested: &[String]) -> Result<Assets> {
    let manifest_path = cache.join("manifest.json");
    let manifest_bytes = fs::read(&manifest_path)?;
    ensure!(
        manifest_bytes.len() <= 32 * 1024 * 1024,
        "oversized asset manifest"
    );
    let manifest: Manifest = serde_json::from_slice(&manifest_bytes)?;
    ensure!(
        manifest.format == FORMAT
            && manifest.source_key == source_key
            && manifest.requested == requested,
        "animation asset cache identity mismatch"
    );
    let canonical = cache.canonicalize()?;
    let mut total_bytes = manifest_bytes.len() as u64;
    let mut content = sha1_smol::Sha1::new();
    for (name, expected) in &manifest.files {
        validate_path(name)?;
        let path = cache.join(name).canonicalize()?;
        ensure!(
            path.starts_with(&canonical),
            "asset cache path escapes its directory"
        );
        let actual = hash_file(&path)?;
        ensure!(
            actual.bytes == expected.bytes && actual.sha1 == expected.sha1,
            "animation asset cache content mismatch"
        );
        total_bytes = total_bytes
            .checked_add(actual.bytes)
            .context("asset cache size overflow")?;
        if name.starts_with("raw/") {
            content.update(name.as_bytes());
            content.update(actual.sha1.as_bytes());
        }
    }
    ensure!(
        content.digest().to_string() == manifest.resource_content_id,
        "animation asset provenance mismatch"
    );
    let mut clips = BTreeMap::new();
    let mut skeleton = None;
    let mut model_skeletons = BTreeMap::new();
    let mut model_hitboxes = BTreeMap::new();
    for resource in requested {
        let text_path = format!("data/{resource}.kv3");
        ensure!(
            manifest.files.contains_key(&text_path)
                && manifest.files.contains_key(&format!("raw/{resource}_c")),
            "animation asset cache is missing a requested resource"
        );
        let text = fs::read_to_string(cache.join(text_path))?;
        if resource.ends_with(".vnmclip") {
            clips.insert(
                resource.clone(),
                Clip::from_kv3(&text)
                    .with_context(|| format!("decoding animation clip {resource}"))?,
            );
        } else if resource.ends_with(".vmdl") {
            let hitbox_path = format!("data/{resource}.hitboxes.json");
            ensure!(
                manifest.files.contains_key(&hitbox_path),
                "model hitbox cache is missing"
            );
            model_hitboxes.insert(
                resource_id(resource)?,
                serde_json::from_slice(&fs::read(cache.join(hitbox_path))?)?,
            );
            model_skeletons.insert(
                resource_id(resource)?,
                model_skeleton_refs(&kv3_text::parse(&text)?)?,
            );
        } else if resource == SKELETON {
            skeleton = Some(kv3_text::parse(&text)?);
        }
    }
    Ok(Assets {
        clips,
        model_skeletons,
        model_hitboxes,
        skeleton: skeleton.context("worldmodel skeleton is missing")?,
        resource_content_id: manifest.resource_content_id,
        total_bytes,
    })
}

fn model_skeleton_refs(value: &serde_json::Value) -> Result<Vec<String>> {
    let Some(refs) = value.get("m_vecNmSkeletonRefs") else {
        return Ok(Vec::new());
    };
    refs.as_array()
        .context("invalid model skeleton references")?
        .iter()
        .map(|value| {
            let path = value.as_str().context("invalid model skeleton resource")?;
            validate_path(path)?;
            ensure!(
                path.ends_with(".vnmskel"),
                "invalid model skeleton resource kind"
            );
            Ok(path.to_owned())
        })
        .collect()
}

fn matching_model_paths(output: &str, handles: &BTreeSet<u64>) -> Result<Vec<String>> {
    let mut found = BTreeMap::new();
    for line in output.lines() {
        let Some(path) = line
            .split_whitespace()
            .next()
            .and_then(|p| p.strip_suffix(".vmdl_c"))
        else {
            continue;
        };
        let path = format!("{path}.vmdl");
        let id = resource_id(&path)?;
        if handles.contains(&id) {
            ensure!(
                found.insert(id, path).is_none(),
                "ambiguous model resource hash"
            );
        }
    }
    ensure!(
        found.len() == handles.len(),
        "recorded model resources are unavailable in installed game"
    );
    Ok(found.into_values().collect())
}

/// Resolve recorded model handles against the source-bound installed VPK catalog once.
fn model_paths(
    root: &Path,
    pak: &Path,
    vrf: &Path,
    handles: &BTreeSet<u64>,
) -> Result<(Vec<String>, u64)> {
    if handles.is_empty() {
        return Ok((Vec::new(), 0));
    }
    let mut hash = sha1_smol::Sha1::new();
    hash.update(hash_file(pak)?.sha1.as_bytes());
    hash.update(hash_file(vrf)?.sha1.as_bytes());
    let dir = root.join("analysis").join("animation-model-catalogs");
    fs::create_dir_all(&dir)?;
    let cache = dir.join(format!("{}.txt", hash.digest()));
    let output = if cache.try_exists()? {
        fs::read_to_string(&cache)?
    } else {
        let output = run(
            vrf,
            Command::new(vrf)
                .arg("-i")
                .arg(pak)
                .arg("--vpk_list")
                .arg("-e")
                .arg("vmdl_c"),
        )?;
        matching_model_paths(&output, handles)?;
        let staging = tempfile::NamedTempFile::new_in(&dir)?;
        fs::write(staging.path(), &output)?;
        match staging.persist_noclobber(&cache) {
            Ok(_) => (),
            Err(error) if cache.is_file() => {
                let _ = error;
            }
            Err(error) => return Err(error.error).context("publishing model catalog"),
        }
        output
    };
    Ok((matching_model_paths(&output, handles)?, output.len() as u64))
}

pub fn prepare(root: &Path, game: &Path, vrf: &Path, resource_paths: &[String]) -> Result<Assets> {
    prepare_with_models(root, game, vrf, resource_paths, &BTreeSet::new())
}

pub fn prepare_with_models(
    root: &Path,
    game: &Path,
    vrf: &Path,
    resource_paths: &[String],
    model_handles: &BTreeSet<u64>,
) -> Result<Assets> {
    prepare_with_model_hitboxes(
        root,
        game,
        vrf,
        resource_paths,
        model_handles,
        &BTreeSet::new(),
    )
}

pub fn prepare_with_model_hitboxes(
    root: &Path,
    game: &Path,
    vrf: &Path,
    resource_paths: &[String],
    model_handles: &BTreeSet<u64>,
    hitbox_handles: &BTreeSet<u64>,
) -> Result<Assets> {
    ensure!(
        hitbox_handles.is_subset(model_handles),
        "hitbox models must be recorded model dependencies"
    );
    let pak = crate::radar::pak_path(game);
    let (models, catalog_bytes) = model_paths(root, &pak, vrf, model_handles)?;
    let mut resources = resource_paths.to_vec();
    resources.extend(models);
    let mut assets = prepare_resources(root, game, vrf, &resources, hitbox_handles)?;
    assets.total_bytes += catalog_bytes;
    Ok(assets)
}

fn prepare_resources(
    root: &Path,
    game: &Path,
    vrf: &Path,
    resource_paths: &[String],
    hitbox_handles: &BTreeSet<u64>,
) -> Result<Assets> {
    let mut requested = BTreeSet::new();
    for resource in resource_paths {
        validate_path(resource)?;
        let resource = resource.strip_suffix("_c").unwrap_or(resource);
        if resource.ends_with(".vnmclip")
            || resource.ends_with(".vnmskel")
            || resource.ends_with(".vmdl")
        {
            requested.insert(resource.to_owned());
        }
    }
    requested.insert(SKELETON.to_owned());
    let requested: Vec<_> = requested.into_iter().collect();
    let pak = crate::radar::pak_path(game);
    let mut hash = sha1_smol::Sha1::new();
    hash.update(&FORMAT.to_le_bytes());
    hash.update(hash_file(&pak)?.sha1.as_bytes());
    hash.update(hash_file(vrf)?.sha1.as_bytes());
    hash.update(&serde_json::to_vec(&requested)?);
    hash.update(&serde_json::to_vec(hitbox_handles)?);
    let source_key = hash.digest().to_string();
    let parent = root.join("analysis").join("animation-assets");
    fs::create_dir_all(&parent)?;
    let cache = parent.join(&source_key);
    if cache.join("manifest.json").try_exists()? {
        return load(&cache, &source_key, &requested);
    }
    let staging = tempfile::Builder::new()
        .prefix(".assets-")
        .tempdir_in(&parent)?;
    let raw = staging.path().join("raw");
    fs::create_dir(&raw)?;
    run(
        vrf,
        Command::new(vrf)
            .arg("-i")
            .arg(&pak)
            .arg("-f")
            .arg(selectors(&requested)?)
            .arg("-e")
            .arg("vnmclip_c,vnmskel_c,vmdl_c")
            .arg("-o")
            .arg(&raw),
    )?;
    for resource in &requested {
        ensure!(
            raw.join(format!("{resource}_c")).is_file(),
            "recorded animation asset is unavailable: {resource}"
        );
    }
    let output = run(
        vrf,
        Command::new(vrf)
            .arg("-i")
            .arg(&raw)
            .arg("--recursive")
            .arg("-b")
            .arg("DATA")
            .arg("--threads")
            .arg("1"),
    )?;
    let raw_canonical = raw.canonicalize()?;
    let mut found = BTreeSet::new();
    for (path, text) in data_chunks(&output)? {
        let canonical = path
            .canonicalize()
            .context("decoder returned a missing resource path")?;
        ensure!(
            canonical.starts_with(&raw_canonical),
            "decoder resource path escapes extraction directory"
        );
        let relative = canonical
            .strip_prefix(&raw_canonical)?
            .to_string_lossy()
            .replace('\\', "/");
        validate_path(&relative)?;
        let resource = relative
            .strip_suffix("_c")
            .context("unexpected compiled resource name")?;
        if requested
            .binary_search_by(|p| p.as_str().cmp(resource))
            .is_err()
        {
            continue;
        }
        ensure!(
            found.insert(resource.to_owned()),
            "duplicate resource DATA output"
        );
        if resource.ends_with(".vnmclip") {
            Clip::from_kv3(text).with_context(|| format!("decoding animation clip {resource}"))?;
        } else {
            if resource.ends_with(".vmdl") {
                model_skeleton_refs(&kv3_text::parse(text)?)?;
            }
            kv3_text::parse(text)
                .with_context(|| format!("decoding animation skeleton {resource}"))?;
        }
        let dest = staging.path().join("data").join(format!("{resource}.kv3"));
        fs::create_dir_all(dest.parent().context("asset output has no parent")?)?;
        fs::write(dest, text)?;
    }
    ensure!(
        found.len() == requested.len(),
        "asset decoder omitted requested DATA blocks"
    );
    let models: BTreeSet<_> = requested
        .iter()
        .filter(|path| path.ends_with(".vmdl"))
        .collect();
    if !models.is_empty() {
        let mesh_output = run(
            vrf,
            Command::new(vrf)
                .arg("-i")
                .arg(&raw)
                .arg("--recursive")
                .arg("-e")
                .arg("vmdl_c")
                .arg("-b")
                .arg("MDAT")
                .arg("--threads")
                .arg("1"),
        )?;
        let mut found_models = BTreeSet::new();
        for (path, section) in resource_sections(&mesh_output)? {
            let canonical = path
                .canonicalize()
                .context("mesh decoder returned a missing path")?;
            ensure!(
                canonical.starts_with(&raw_canonical),
                "mesh decoder path escapes extraction directory"
            );
            let relative = canonical
                .strip_prefix(&raw_canonical)?
                .to_string_lossy()
                .replace('\\', "/");
            validate_path(&relative)?;
            let model = relative
                .strip_suffix("_c")
                .context("invalid compiled model name")?;
            if !models.iter().any(|path| path.as_str() == model) {
                continue;
            }
            ensure!(
                found_models.insert(model.to_owned()),
                "duplicate model mesh output"
            );
            let hitboxes = if hitbox_handles.contains(&resource_id(model)?) {
                model_hitboxes::parse_meshes(&mesh_blocks(section)?)
                    .with_context(|| format!("decoding model hitbox sets {model}"))?
            } else {
                Vec::new()
            };
            let dest = staging
                .path()
                .join("data")
                .join(format!("{model}.hitboxes.json"));
            fs::write(dest, serde_json::to_vec(&hitboxes)?)?;
        }
        ensure!(
            found_models.len() == models.len(),
            "mesh decoder omitted a requested model"
        );
    }
    let mut files = BTreeMap::new();
    collect_files(staging.path(), staging.path(), &mut files)?;
    let mut content = sha1_smol::Sha1::new();
    for (name, digest) in &files {
        if name.starts_with("raw/") {
            content.update(name.as_bytes());
            content.update(digest.sha1.as_bytes());
        }
    }
    let manifest = Manifest {
        format: FORMAT,
        source_key: source_key.clone(),
        requested: requested.clone(),
        files,
        resource_content_id: content.digest().to_string(),
    };
    fs::write(
        staging.path().join("manifest.json"),
        serde_json::to_vec(&manifest)?,
    )?;
    // Validate before publishing the complete shared cache.
    let assets = load(staging.path(), &source_key, &requested)?;
    let published = if cache.try_exists()? && !cache.join("manifest.json").try_exists()? {
        publish_files(staging.path(), &cache, &manifest)
            .and_then(|()| load(&cache, &source_key, &requested))
    } else {
        match fs::rename(staging.path(), &cache) {
            Ok(()) => Ok(assets),
            Err(_) if cache.join("manifest.json").try_exists()? => {
                load(&cache, &source_key, &requested)
            }
            // Some Windows environments reject renaming populated trees even though
            // individual file publication succeeds. Never publish the manifest early.
            Err(error) if cache.is_dir() || (cfg!(windows) && error.raw_os_error() == Some(5)) => {
                publish_files(staging.path(), &cache, &manifest)
                    .and_then(|()| load(&cache, &source_key, &requested))
            }
            Err(error) => Err(error).context("publishing animation asset cache"),
        }
    };
    published.map_err(|error| {
        let retained = staging.keep();
        error.context(format!(
            "animation asset publication failed; verified staging retained at {}",
            retained.display()
        ))
    })
}

/// Publish complete files independently, then atomically commit the manifest.
/// An interrupted destination without a manifest is resumable, never a valid cache.
fn publish_files(staging: &Path, cache: &Path, manifest: &Manifest) -> Result<()> {
    fs::create_dir_all(cache)?;
    ensure!(
        !fs::symlink_metadata(cache)?.file_type().is_symlink(),
        "asset cache cannot be a symlink"
    );
    if cache.join("manifest.json").try_exists()? {
        return Ok(());
    }
    let canonical = cache.canonicalize()?;
    for (name, expected) in &manifest.files {
        validate_path(name)?;
        let destination = cache.join(name);
        let parent = destination.parent().context("asset file has no parent")?;
        fs::create_dir_all(parent)?;
        ensure!(
            parent.canonicalize()?.starts_with(&canonical),
            "asset output escapes cache directory"
        );
        if destination.try_exists()? {
            ensure!(
                !fs::symlink_metadata(&destination)?.file_type().is_symlink(),
                "asset file cannot be a symlink"
            );
            let current = hash_file(&destination)?;
            if current.bytes == expected.bytes && current.sha1 == expected.sha1 {
                continue;
            }
        }
        let temporary = tempfile::NamedTempFile::new_in(parent)?;
        fs::copy(staging.join(name), temporary.path())?;
        let copied = hash_file(temporary.path())?;
        ensure!(
            copied.bytes == expected.bytes && copied.sha1 == expected.sha1,
            "staged asset content changed during publication"
        );
        temporary
            .persist(&destination)
            .map_err(|e| e.error)
            .context("publishing animation asset file")?;
    }
    let temporary = tempfile::NamedTempFile::new_in(cache)?;
    fs::write(temporary.path(), serde_json::to_vec(manifest)?)?;
    match temporary.persist_noclobber(cache.join("manifest.json")) {
        Ok(_) => Ok(()),
        Err(_) if cache.join("manifest.json").try_exists()? => Ok(()),
        Err(error) => Err(error.error).context("publishing animation asset manifest"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mesh_blocks_preserve_all_models_and_empty_weapon_geometry() {
        let output = "[1/2] /tmp/player.vmdl_c\nmetadata\n--- Data for block \"MDAT\" ---\n<!-- kv3 text -->\n{\nmesh=1\n}\n--- Data for block \"MDAT\" ---\n<!-- kv3 text -->\n{\nmesh=2\n}\n[2/2] /tmp/weapon.vmdl_c\nmetadata only\n";
        let sections = resource_sections(output).unwrap();
        assert_eq!(sections.len(), 2);
        let meshes = mesh_blocks(sections[0].1).unwrap();
        assert_eq!(meshes.len(), 2);
        assert_eq!(meshes[0]["mesh"], 1);
        assert_eq!(meshes[1]["mesh"], 2);
        assert!(mesh_blocks(sections[1].1).unwrap().is_empty());
        assert!(mesh_blocks("--- Data for block \"MDAT\" ---\n<!-- kv3 text -->\n{").is_err());
    }

    #[test]
    fn interrupted_file_publication_resumes_and_commits_manifest_last() {
        let root = tempfile::tempdir().unwrap();
        let staging = root.path().join("staging");
        let cache = root.path().join("cache");
        fs::create_dir(&staging).unwrap();
        fs::create_dir(staging.join("raw")).unwrap();
        fs::write(staging.join("raw/a"), b"first").unwrap();
        fs::write(staging.join("raw/b"), b"second").unwrap();
        let mut files = BTreeMap::new();
        collect_files(&staging, &staging, &mut files).unwrap();
        let manifest = Manifest {
            format: FORMAT,
            source_key: "test".into(),
            requested: vec![],
            files,
            resource_content_id: "test".into(),
        };
        fs::write(staging.join("raw/b"), b"changed source").unwrap();
        assert!(publish_files(&staging, &cache, &manifest).is_err());
        assert_eq!(fs::read(cache.join("raw/a")).unwrap(), b"first");
        assert!(!cache.join("manifest.json").exists());
        assert!(load(&cache, "test", &[]).is_err());
        fs::write(staging.join("raw/b"), b"second").unwrap();
        fs::write(cache.join("raw/b"), b"incomplete prior output").unwrap();
        publish_files(&staging, &cache, &manifest).unwrap();
        assert_eq!(fs::read(cache.join("raw/b")).unwrap(), b"second");
        let committed = fs::read(cache.join("manifest.json")).unwrap();
        let parsed: Manifest = serde_json::from_slice(&committed).unwrap();
        assert_eq!(parsed.files.len(), 2);
        // A competing publisher must leave a completed manifest alone; the caller validates it.
        fs::write(staging.join("raw/a"), b"changed after commit").unwrap();
        publish_files(&staging, &cache, &manifest).unwrap();
        assert_eq!(fs::read(cache.join("manifest.json")).unwrap(), committed);
        assert_eq!(fs::read(cache.join("raw/a")).unwrap(), b"first");
    }

    #[test]
    fn model_catalog_and_skeleton_references_are_exact() {
        let path = "weapons/models/knife/knife_default_t/weapon_knife_default_t.vmdl";
        let id = resource_id(path).unwrap();
        assert_eq!(id, 7398392065489137260);
        let handles = BTreeSet::from([id]);
        let output = format!("{path}_c CRC:abcd size:12\nignored metadata\n");
        assert_eq!(matching_model_paths(&output, &handles).unwrap(), [path]);
        assert!(matching_model_paths("", &handles).is_err());
        assert!(matching_model_paths(&(output.clone() + &output), &handles).is_err());
        let refs = kv3_text::parse("{ m_vecNmSkeletonRefs = [resource:\"animation/skeletons/weapons/knife_default_t.vnmskel\"] }").unwrap();
        assert_eq!(
            model_skeleton_refs(&refs).unwrap(),
            ["animation/skeletons/weapons/knife_default_t.vnmskel"]
        );
        assert!(
            model_skeleton_refs(&serde_json::json!({"m_vecNmSkeletonRefs":["../x.vnmskel"]}))
                .is_err()
        );
    }

    #[test]
    fn resource_ids_match_recorded_graph_and_compiled_skeleton_reference() {
        let graph = resource_id("animation/graphs/worldmodel/worldmodel.vnmgraph").unwrap();
        assert_eq!(graph, 0x87ce5ff43c25ba7d);
        assert_eq!(graph as u32, 1009105533);
        assert_eq!(
            resource_id("ANIMATION/GRAPHS/WORLDMODEL/WORLDMODEL.VNMGRAPH").unwrap(),
            graph
        );
        assert_eq!(
            resource_id("animation/skeletons/characters/worldmodel.vnmskel").unwrap(),
            0xa3193fb3bc4ad00a
        );
        assert!(resource_id("../worldmodel.vnmgraph").is_err());
    }

    #[test]
    fn paths_selectors_and_batch_boundaries_are_checked() {
        for bad in [
            "../x", "/x", "C:/x", "a\\b", "a,b", "a//b", "a/./b", "a\nb", "a:*",
        ] {
            assert!(validate_path(bad).is_err(), "{bad:?}");
        }
        let paths: Vec<_> = (0..1000)
            .map(|i| format!("animation/anims/world/long_directory/clip_{i:04}.vnmclip"))
            .collect();
        let filter = selectors(&paths).unwrap();
        assert!(filter.len() <= 20_000);
        for path in &paths {
            assert!(filter
                .split(',')
                .any(|selector| format!("{path}_c") == selector
                    || (selector.ends_with('/') && path.starts_with(selector))));
        }
        let output = "[1/2] /tmp/one.vnmclip_c\nmetadata\n<!-- kv3 text -->\n{\na = 1\n}\n[2/2] /tmp/two.vnmclip_c\n<!-- kv3 text -->\n{\na = 2\n}\n";
        let chunks = data_chunks(output).unwrap();
        assert_eq!(chunks.len(), 2);
        assert_eq!(kv3_text::parse(chunks[0].1).unwrap()["a"], 1);
        assert_eq!(kv3_text::parse(chunks[1].1).unwrap()["a"], 2);
        assert!(data_chunks("[1/1] /tmp/x\n<!-- kv3 text -->\n{").is_err());
    }
}
