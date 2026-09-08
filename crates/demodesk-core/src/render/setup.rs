//! Downloads the third-party binaries into `<tools_dir>`:
//!   hlae/     latest advancedfx release (github.com/advancedfx/advancedfx)
//!   ffmpeg/   BtbN static win64 build (github.com/BtbN/FFmpeg-Builds)
//!   vrf/      Source2Viewer-CLI (github.com/ValveResourceFormat), reads radar images out of the game's VPK

use anyhow::{anyhow, Context, Result};
use serde::Deserialize;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

pub type Log<'a> = &'a mut dyn FnMut(String);

/// HTTPS through the OS trust store (Windows certificate store), so a
/// corporate / antivirus proxy that re-signs TLS still works.
fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .user_agent("DemoDesk")
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .tls_config(TlsConfig::builder().root_certs(RootCerts::PlatformVerifier).build())
        .build()
        .into()
}

fn download(url: &str, dest: &Path, log: Log) -> Result<()> {
    fs::create_dir_all(dest.parent().unwrap())?;
    let resp = agent().get(url).call().with_context(|| format!("GET {url}"))?;
    let total = resp.headers().get("Content-Length").and_then(|v| v.to_str().ok()).and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_body().into_reader();
    let mut file = fs::File::create(dest)?;
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    let mut last_pct = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        std::io::Write::write_all(&mut file, &buf[..n])?;
        done += n as u64;
        if let Some(t) = total {
            let pct = (done * 100 / t.max(1)) as u32;
            if pct >= last_pct + 10 {
                last_pct = pct;
                log(format!("  {}% of {:.1} MB", pct, t as f64 / 1_048_576.0));
            }
        }
    }
    if total.is_some_and(|expected| done != expected) { return Err(anyhow!("incomplete download from {url}: received {done} bytes, expected {total:?}")); }
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let rel = entry.enclosed_name().ok_or_else(|| anyhow!("archive contains an unsafe path"))?;
        let out = dest.join(rel);
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
    }
    Ok(())
}

#[derive(Deserialize)]
struct GithubRelease {
    tag_name: String,
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Recover a replacement interrupted after the old installation was moved aside.
fn recover_install(dir: &Path) -> Result<()> {
    let backup = dir.with_extension("previous");
    if backup.exists() && !dir.exists() { fs::rename(&backup, dir).context("restore previous installation")?; }
    if backup.exists() { fs::remove_dir_all(&backup).context("remove previous installation")?; }
    Ok(())
}

fn publish_install(dir: &Path, staged: &Path) -> Result<()> {
    let backup = dir.with_extension("previous");
    let existed = dir.exists();
    if existed { fs::rename(dir, &backup).context("tool may be in use; close it before updating")?; }
    if let Err(error) = fs::rename(staged, dir) {
        if existed { fs::rename(&backup, dir).context("replacement failed; previous installation is in the .previous directory")?; }
        return Err(error.into());
    }
    // A failed cleanup is recoverable on the next setup; the new installation is valid.
    if existed { let _ = fs::remove_dir_all(backup); }
    Ok(())
}

/// Fully extract and validate before replacing a working installation.
fn install_zip(dir: &Path, url: &str, tag: &str, valid: fn(&Path) -> bool, log: Log) -> Result<()> {
    recover_install(dir)?;
    let work = dir.with_extension("installing");
    if work.exists() { fs::remove_dir_all(&work)?; }
    let zip = work.join("download.zip");
    download(url, &zip, log)?;
    install_archive(dir, &work, tag, url, valid)
}

fn install_archive(dir: &Path, work: &Path, tag: &str, url: &str, valid: fn(&Path) -> bool) -> Result<()> {
    let staged = work.join("extracted");
    extract_zip(&work.join("download.zip"), &staged)?;
    if !valid(&staged) { return Err(anyhow!("archive for {} is missing required tool files", dir.display())); }
    fs::write(staged.join("install-info.json"), serde_json::json!({ "tag": tag, "url": url, "installedAt": chrono::Utc::now().to_rfc3339() }).to_string())?;
    publish_install(dir, &staged)?;
    let _ = fs::remove_dir_all(work);
    Ok(())
}

fn install_release(repo: &str, dir: &Path, select: fn(&GithubRelease) -> Result<&GithubAsset>, valid: fn(&Path) -> bool, log: Log) -> Result<()> {
    for attempt in 0..2 {
        let release = release_for_tool(repo, select)?;
        let asset = select(&release)?;
        log(format!("downloading {repo} {} from {}", release.tag_name, asset.browser_download_url));
        match install_zip(dir, &asset.browser_download_url, &release.tag_name, valid, log) {
            Err(error) if attempt == 0 && matches!(error.downcast_ref::<ureq::Error>(), Some(ureq::Error::StatusCode(404 | 410))) => {
                log("release asset was replaced; refreshing release information…".into());
            }
            result => return result,
        }
    }
    unreachable!()
}

fn hlae_installed(dir: &Path) -> bool {
    dir.join("HLAE.exe").is_file() && dir.join("x64/AfxHookSource2.dll").is_file()
}

fn hlae_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    unique_asset(release, |name| name.to_ascii_lowercase().starts_with("hlae_") && name.ends_with(".zip"), "HLAE portable ZIP")
}

fn unique_asset<'a>(release: &'a GithubRelease, matches: impl Fn(&str) -> bool, description: &str) -> Result<&'a GithubAsset> {
    let mut candidates = release.assets.iter().filter(|a| matches(&a.name));
    let asset = candidates.next().ok_or_else(|| anyhow!("no {description} in release {}; available assets: {}", release.tag_name, release.assets.iter().map(|a| a.name.as_str()).collect::<Vec<_>>().join(", ")))?;
    if candidates.next().is_some() { return Err(anyhow!("multiple {description} assets in release {}", release.tag_name)); }
    Ok(asset)
}

pub fn install_hlae(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("hlae");
    recover_install(&dir)?;
    if !force && hlae_installed(&dir) {
        log("HLAE already installed".into());
        return Ok(dir);
    }
    install_release("advancedfx/advancedfx", &dir, hlae_asset, hlae_installed, log)?;
    log("HLAE installed".into());
    Ok(dir)
}

fn ffmpeg_installed(dir: &Path) -> bool {
    super::paths::find_file(dir, "ffmpeg.exe", 4)
        .is_some_and(|exe| exe.parent().is_some_and(|bin| bin.join("ffprobe.exe").is_file()))
}

fn ffmpeg_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    unique_asset(release, |name| name.starts_with("ffmpeg-") && name.ends_with("-win64-gpl.zip"), "static win64 GPL FFmpeg ZIP")
}

pub fn install_ffmpeg(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("ffmpeg");
    recover_install(&dir)?;
    if !force && ffmpeg_installed(&dir) {
        log("FFmpeg already installed".into());
        return Ok(dir);
    }
    install_release("BtbN/FFmpeg-Builds", &dir, ffmpeg_asset, ffmpeg_installed, log)?;
    log("FFmpeg installed".into());
    Ok(dir)
}

/// BtbN's rolling tag and GitHub's latest release are different publications.
fn release_for_tool(repo: &str, select: fn(&GithubRelease) -> Result<&GithubAsset>) -> Result<GithubRelease> {
    let sources: &[&str] = if repo == "BtbN/FFmpeg-Builds" { &["tags/latest", "latest"] } else { &["latest"] };
    let mut errors = Vec::new();
    for source in sources {
        let url = format!("https://api.github.com/repos/{repo}/releases/{source}");
        let fetched: Result<GithubRelease> = (|| {
            let release = agent().get(&url).header("Accept", "application/vnd.github+json")
                .call()?.body_mut().read_json()?;
            select(&release)?;
            Ok(release)
        })();
        match fetched {
            Ok(release) => return Ok(release),
            Err(error) => errors.push(format!("{url}: {error:#}")),
        }
    }
    Err(anyhow!("could not resolve a compatible tool download: {}", errors.join("; ")))
}

pub fn vrf_exe_name() -> &'static str {
    if cfg!(windows) {
        "Source2Viewer-CLI.exe"
    } else {
        "Source2Viewer-CLI"
    }
}

fn vrf_installed(dir: &Path) -> bool { dir.join(vrf_exe_name()).is_file() }

fn vrf_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    let wanted = if cfg!(windows) { "cli-windows-x64.zip" } else { "cli-linux-x64.zip" };
    unique_asset(release, |name| name == wanted, wanted)
}

pub fn install_vrf(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("vrf");
    let exe = dir.join(vrf_exe_name());
    recover_install(&dir)?;
    if !force && vrf_installed(&dir) {
        log("Source 2 Viewer CLI already installed".into());
        return Ok(exe);
    }
    install_release("ValveResourceFormat/ValveResourceFormat", &dir, vrf_asset, vrf_installed, log)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&exe, fs::Permissions::from_mode(0o755));
    }
    log("Source 2 Viewer CLI installed".into());
    Ok(exe)
}

/// Tells HLAE where ffmpeg lives: <hlaeDir>/ffmpeg/ffmpeg.ini with [Ffmpeg] Path=…
pub fn register_ffmpeg_with_hlae(hlae_exe: &Path, ffmpeg_exe: &Path) -> Result<()> {
    let ini_dir = hlae_exe.parent().unwrap().join("ffmpeg");
    fs::create_dir_all(&ini_dir)?;
    fs::write(ini_dir.join("ffmpeg.ini"), format!("[Ffmpeg]\nPath={}\n", ffmpeg_exe.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn release(names: &[&str]) -> GithubRelease {
        GithubRelease { tag_name: "autobuild".into(),
            assets: names.iter().map(|name| GithubAsset { name: (*name).into(), browser_download_url: format!("https://example.invalid/{name}") }).collect() }
    }

    #[test]
    fn ffmpeg_selection_accepts_versioned_names_and_rejects_other_builds() {
        let name = "ffmpeg-N-126475-g35b7df64a0-win64-gpl.zip";
        let current = release(&["ffmpeg-N-126475-win64-gpl-shared.zip", "ffmpeg-N-126475-win64-lgpl.zip",
            "ffmpeg-N-126475-winarm64-gpl.zip", "ffmpeg-n9.0.1-win64-gpl-9.0.zip", name]);
        assert_eq!(ffmpeg_asset(&current).unwrap().name, name);
        assert!(ffmpeg_asset(&release(&["ffmpeg-master-latest-win64-gpl.zip"])).is_ok());
        assert!(ffmpeg_asset(&release(&["ffmpeg-master-latest-win64-lgpl.zip"])).is_err());
        assert!(ffmpeg_asset(&release(&[name, "ffmpeg-master-latest-win64-gpl.zip"])).is_err());
    }

    #[test]
    fn ffmpeg_install_requires_both_executables() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!ffmpeg_installed(dir.path()));
        let bin = dir.path().join("version/bin");
        fs::create_dir_all(&bin).unwrap();
        fs::write(bin.join("ffmpeg.exe"), b"fixture").unwrap();
        assert!(!ffmpeg_installed(dir.path()));
        fs::write(bin.join("ffprobe.exe"), b"fixture").unwrap();
        assert!(ffmpeg_installed(dir.path()));
    }
    #[test]
    fn tool_assets_are_unambiguous_and_platform_specific() {
        assert_eq!(hlae_asset(&release(&["HLAE_setup.exe", "hlae_2_191_1.zip.asc", "hlae_2_191_1.zip"])).unwrap().name, "hlae_2_191_1.zip");
        assert!(hlae_asset(&release(&["source.zip", "HLAE_setup.exe"])).is_err());
        let platform = if cfg!(windows) { "cli-windows-x64.zip" } else { "cli-linux-x64.zip" };
        assert_eq!(vrf_asset(&release(&["cli-linux-arm64.zip", "gui-windows-x64.zip", platform])).unwrap().name, platform);
        assert!(vrf_asset(&release(&["gui-windows-x64.zip"])).is_err());
    }

    #[test]
    fn failed_replacement_preserves_old_tools_and_interrupted_swap_recovers() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hlae");
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("old"), b"working").unwrap();
        assert!(publish_install(&dir, &root.path().join("missing-stage")).is_err());
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        fs::rename(&dir, dir.with_extension("previous")).unwrap();
        recover_install(&dir).unwrap();
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        let staged = root.path().join("new");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("new"), b"ready").unwrap();
        publish_install(&dir, &staged).unwrap();
        assert!(dir.join("new").is_file());
        assert!(!dir.join("old").exists());
        assert!(!dir.with_extension("previous").exists());
    }

    #[test]
    fn corrupt_or_incomplete_archives_leave_working_installation_untouched() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("hlae");
        let work = dir.with_extension("installing");
        fs::create_dir(&dir).unwrap();
        fs::create_dir(&work).unwrap();
        fs::write(dir.join("old"), b"working").unwrap();
        fs::write(work.join("download.zip"), b"truncated download").unwrap();
        assert!(install_archive(&dir, &work, "test", "test", hlae_installed).is_err());
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        let mut zip = zip::ZipWriter::new(fs::File::create(work.join("download.zip")).unwrap());
        zip.start_file("README.txt", zip::write::SimpleFileOptions::default()).unwrap();
        zip.write_all(b"missing binaries").unwrap();
        zip.finish().unwrap();
        assert!(install_archive(&dir, &work, "test", "test", hlae_installed).is_err());
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
    }

}
