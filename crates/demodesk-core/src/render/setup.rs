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
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let Some(rel) = entry.enclosed_name() else { continue };
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
    prerelease: bool,
    assets: Vec<GithubAsset>,
}
#[derive(Deserialize)]
struct GithubAsset {
    name: String,
    browser_download_url: String,
}

/// Download a zip and unpack it into `dir` (replacing whatever was there),
/// leaving `install-info.json` with the release tag.
fn install_zip(tools_dir: &Path, dir: &Path, url: &str, file_name: &str, tag: &str, log: Log) -> Result<()> {
    let zip = tools_dir.join("downloads").join(file_name);
    download(url, &zip, log)?;
    let _ = fs::remove_dir_all(dir);
    extract_zip(&zip, dir)?;
    fs::write(dir.join("install-info.json"), serde_json::json!({ "tag": tag, "installedAt": chrono::Utc::now().to_rfc3339() }).to_string())?;
    Ok(())
}

pub fn install_hlae(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("hlae");
    if !force && dir.join("HLAE.exe").is_file() {
        log("HLAE already installed".into());
        return Ok(dir);
    }
    log("fetching latest HLAE release…".into());
    let release = latest_release("advancedfx/advancedfx")?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.name.to_lowercase().ends_with(".zip") && !a.name.to_lowercase().contains("source"))
        .ok_or_else(|| anyhow!("no HLAE zip asset found"))?;
    log(format!("downloading HLAE {} from {}", release.tag_name, asset.browser_download_url));
    install_zip(tools_dir, &dir, &asset.browser_download_url, &asset.name, &release.tag_name, log)?;
    log(format!("HLAE {} installed", release.tag_name));
    Ok(dir)
}

pub fn install_ffmpeg(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("ffmpeg");
    if !force && dir.is_dir() {
        log("FFmpeg already installed".into());
        return Ok(dir);
    }
    let url = "https://github.com/BtbN/FFmpeg-Builds/releases/latest/download/ffmpeg-master-latest-win64-gpl.zip";
    log(format!("downloading FFmpeg (BtbN win64 gpl) from {url}"));
    install_zip(tools_dir, &dir, url, "ffmpeg-win64-gpl.zip", "latest", log)?;
    log("FFmpeg installed".into());
    Ok(dir)
}

/// Latest non-prerelease of a GitHub repo.
fn latest_release(repo: &str) -> Result<GithubRelease> {
    let releases: Vec<GithubRelease> = agent().get(format!("https://api.github.com/repos/{repo}/releases?per_page=10")).header("Accept", "application/vnd.github+json").call()?.body_mut().read_json()?;
    releases.into_iter().find(|r| !r.prerelease).ok_or_else(|| anyhow!("no release found for {repo}"))
}

pub fn vrf_exe_name() -> &'static str {
    if cfg!(windows) {
        "Source2Viewer-CLI.exe"
    } else {
        "Source2Viewer-CLI"
    }
}

pub fn install_vrf(tools_dir: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = tools_dir.join("vrf");
    let exe = dir.join(vrf_exe_name());
    if !force && exe.is_file() {
        log("Source 2 Viewer CLI already installed".into());
        return Ok(exe);
    }
    log("fetching latest Source 2 Viewer release…".into());
    let release = latest_release("ValveResourceFormat/ValveResourceFormat")?;
    let wanted = if cfg!(windows) { "cli-windows-x64.zip" } else { "cli-linux-x64.zip" };
    let asset = release.assets.iter().find(|a| a.name == wanted).ok_or_else(|| anyhow!("no {wanted} asset in release {}", release.tag_name))?;
    log(format!("downloading Source 2 Viewer CLI {} from {}", release.tag_name, asset.browser_download_url));
    install_zip(tools_dir, &dir, &asset.browser_download_url, &asset.name, &release.tag_name, log)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&exe, fs::Permissions::from_mode(0o755));
    }
    log(format!("Source 2 Viewer CLI {} installed", release.tag_name));
    Ok(exe)
}

/// Tells HLAE where ffmpeg lives: <hlaeDir>/ffmpeg/ffmpeg.ini with [Ffmpeg] Path=…
pub fn register_ffmpeg_with_hlae(hlae_exe: &Path, ffmpeg_exe: &Path) -> Result<()> {
    let ini_dir = hlae_exe.parent().unwrap().join("ffmpeg");
    fs::create_dir_all(&ini_dir)?;
    fs::write(ini_dir.join("ffmpeg.ini"), format!("[Ffmpeg]\nPath={}\n", ffmpeg_exe.display()))?;
    Ok(())
}
