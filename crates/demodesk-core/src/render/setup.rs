//! Downloads the third-party binaries into `<tools_dir>`:
//!   hlae/     latest advancedfx release (github.com/advancedfx/advancedfx)
//!   ffmpeg/   BtbN static win64 build (github.com/BtbN/FFmpeg-Builds)
//!   vrf/      Source2Viewer-CLI (github.com/ValveResourceFormat), reads radar images out of the game's VPK

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

mod transport;

pub struct Progress<'a> {
    pub cancel: &'a Arc<AtomicBool>,
    pub report: &'a mut dyn FnMut(String),
}
pub type Log<'a, 'b> = &'a mut Progress<'b>;

impl Progress<'_> {
    fn check(&self) -> Result<()> {
        anyhow::ensure!(!self.cancel.load(Ordering::Relaxed), "Download cancelled");
        Ok(())
    }
}

/// HTTPS through the OS trust store (Windows certificate store), so a
/// corporate / antivirus proxy that re-signs TLS still works.
fn agent() -> ureq::Agent {
    use ureq::tls::{RootCerts, TlsConfig};
    ureq::Agent::config_builder()
        .user_agent("DemoDesk")
        .timeout_global(Some(std::time::Duration::from_secs(600)))
        .timeout_connect(Some(Duration::from_secs(30)))
        .timeout_recv_response(Some(Duration::from_secs(60)))
        .tls_config(
            TlsConfig::builder()
                .root_certs(RootCerts::PlatformVerifier)
                .build(),
        )
        .build()
        .into()
}

fn download(url: &str, dest: &Path, log: Log) -> Result<()> {
    log.check()?;
    fs::create_dir_all(dest.parent().unwrap()).with_context(|| {
        format!(
            "Cannot create download directory {}",
            dest.parent().unwrap().display()
        )
    })?;
    let mut file = fs::File::create(dest)
        .with_context(|| format!("Cannot write download file {}. Check folder permissions or choose another data directory", dest.display()))?;
    let resp = transport::agent(log.cancel.clone())
        .get(url)
        .config()
        .timeout_global(Some(Duration::from_secs(2 * 60 * 60)))
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?;
    let total = resp
        .headers()
        .get("Content-Length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());
    let mut reader = resp.into_body().into_reader();
    copy_download(&mut reader, &mut file, total, log)
        .with_context(|| format!("Downloading {url} to {}", dest.display()))
}

fn copy_download(
    reader: &mut impl Read,
    file: &mut impl std::io::Write,
    total: Option<u64>,
    log: Log,
) -> Result<()> {
    let mut buf = vec![0u8; 1 << 16];
    let mut done = 0u64;
    let mut last_report = None;
    let report = |done: u64| match total {
        Some(t) => format!(
            "  {:.2} / {:.2} MB ({}%)",
            done as f64 / 1_048_576.0,
            t as f64 / 1_048_576.0,
            done.saturating_mul(100) / t.max(1)
        ),
        None => format!("  {:.2} MB downloaded", done as f64 / 1_048_576.0),
    };
    loop {
        log.check()?;
        let n = reader.read(&mut buf)?;
        log.check()?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])?;
        done += n as u64;
        if last_report.is_none_or(|at: Instant| at.elapsed() >= Duration::from_secs(1)) {
            (log.report)(report(done));
            last_report = Some(Instant::now());
        }
    }
    if total.is_some_and(|expected| done != expected) {
        return Err(anyhow!(
            "incomplete download: received {done} bytes, expected {total:?}"
        ));
    }
    (log.report)(report(done));
    Ok(())
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<()> {
    fs::create_dir_all(dest)?;
    let file = fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let rel = entry
            .enclosed_name()
            .ok_or_else(|| anyhow!("archive contains an unsafe path"))?;
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

// ponytail: serialize only replacement/recovery; use per-directory locks if publishing becomes contended.
static INSTALL_COMMIT: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn previous_directory(dir: &Path) -> PathBuf {
    let mut name = dir.file_name().unwrap_or_default().to_os_string();
    name.push(".demodesk-previous");
    dir.with_file_name(name)
}

/// Replacement owns the whole directory, so arbitrary user folders must never be replaced.
fn validate_install_directory(dir: &Path, repo: &str, replacing: bool) -> Result<()> {
    anyhow::ensure!(
        dir.is_absolute()
            && dir.components().count() >= 3
            && !dir
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir)),
        "Choose an absolute tool directory"
    );
    for path in [dir.to_path_buf(), previous_directory(dir)] {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        #[cfg(windows)]
        let link = {
            use std::os::windows::fs::MetadataExt;
            metadata.file_attributes() & 0x400 != 0
        };
        #[cfg(not(windows))]
        let link = metadata.is_symlink();
        anyhow::ensure!(
            metadata.is_dir() && !link,
            "Choose a regular tool directory: {}",
            path.display()
        );
        if (!replacing && path == dir) || fs::read_dir(&path)?.next().is_none() {
            continue;
        }
        let info = fs::read(path.join("install-info.json"))
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
        let prefix = format!("https://github.com/{repo}/releases/download/");
        anyhow::ensure!(
            info.as_ref()
                .and_then(|v| v.get("url"))
                .and_then(|v| v.as_str())
                .is_some_and(|url| url.starts_with(&prefix)),
            "Choose an empty folder or this tool's DemoDesk installation folder: {}",
            path.display()
        );
    }
    Ok(())
}

/// Recover a replacement interrupted after the old installation was moved aside.
fn recover_install(dir: &Path) -> Result<()> {
    let _commit = INSTALL_COMMIT.lock().unwrap();
    let backup = previous_directory(dir);
    if backup.exists() && !dir.exists() {
        fs::rename(&backup, dir).context("restore previous installation")?;
    }
    if backup.exists() {
        fs::remove_dir_all(&backup).context("remove previous installation")?;
    }
    Ok(())
}

fn publish_install(dir: &Path, staged: &Path) -> Result<()> {
    let backup = previous_directory(dir);
    let existed = dir.exists();
    if existed {
        fs::rename(dir, &backup).context("tool may be in use; close it before updating")?;
    }
    if let Err(error) = fs::rename(staged, dir) {
        if existed {
            fs::rename(&backup, dir).context(
                "replacement failed; previous installation is in the .previous directory",
            )?;
        }
        return Err(error.into());
    }
    // A failed cleanup is recoverable on the next setup; the new installation is valid.
    if existed {
        let _ = fs::remove_dir_all(backup);
    }
    Ok(())
}

/// Fully extract and validate before replacing a working installation.
fn install_zip(dir: &Path, url: &str, tag: &str, valid: fn(&Path) -> bool, log: Log) -> Result<()> {
    recover_install(dir)?;
    let parent = dir
        .parent()
        .ok_or_else(|| anyhow!("Invalid tool directory"))?;
    fs::create_dir_all(parent)?;
    let work = tempfile::Builder::new()
        .prefix(".demodesk-install-")
        .tempdir_in(parent)?;
    let zip = work.path().join("download.zip");
    let result = download(url, &zip, log)
        .and_then(|_| install_archive(dir, work.path(), tag, url, valid, log.cancel));
    result
}

fn install_archive(
    dir: &Path,
    work: &Path,
    tag: &str,
    url: &str,
    valid: fn(&Path) -> bool,
    cancel: &AtomicBool,
) -> Result<()> {
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Download cancelled");
    let staged = work.join("extracted");
    extract_zip(&work.join("download.zip"), &staged)?;
    if !valid(&staged) {
        return Err(anyhow!(
            "archive for {} is missing required tool files",
            dir.display()
        ));
    }
    fs::write(staged.join("install-info.json"), serde_json::json!({ "tag": tag, "url": url, "installedAt": chrono::Utc::now().to_rfc3339() }).to_string())?;
    anyhow::ensure!(!cancel.load(Ordering::Relaxed), "Download cancelled");
    let _commit = INSTALL_COMMIT.lock().unwrap();
    let repo = url
        .strip_prefix("https://github.com/")
        .and_then(|url| url.split_once("/releases/download/"))
        .map(|(repo, _)| repo)
        .ok_or_else(|| anyhow!("Unrecognized tool download source"))?;
    validate_install_directory(dir, repo, true)?;
    publish_install(dir, &staged)?;
    let _ = fs::remove_dir_all(work);
    Ok(())
}

fn install_release(
    repo: &str,
    dir: &Path,
    select: fn(&GithubRelease) -> Result<&GithubAsset>,
    valid: fn(&Path) -> bool,
    log: Log,
) -> Result<()> {
    for attempt in 0..2 {
        let release = release_for_tool(repo, select, log)?;
        let asset = select(&release)?;
        match install_zip(
            dir,
            &asset.browser_download_url,
            &release.tag_name,
            valid,
            log,
        ) {
            Err(error)
                if attempt == 0
                    && matches!(
                        error.downcast_ref::<ureq::Error>(),
                        Some(ureq::Error::StatusCode(404 | 410))
                    ) => {}
            result => return result,
        }
    }
    unreachable!()
}

fn hlae_installed(dir: &Path) -> bool {
    dir.join("HLAE.exe").is_file() && dir.join("x64/AfxHookSource2.dll").is_file()
}

fn hlae_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    unique_asset(
        release,
        |name| name.to_ascii_lowercase().starts_with("hlae_") && name.ends_with(".zip"),
        "HLAE portable ZIP",
    )
}

fn unique_asset<'a>(
    release: &'a GithubRelease,
    matches: impl Fn(&str) -> bool,
    description: &str,
) -> Result<&'a GithubAsset> {
    let mut candidates = release.assets.iter().filter(|a| matches(&a.name));
    let asset = candidates.next().ok_or_else(|| {
        anyhow!(
            "no {description} in release {}; available assets: {}",
            release.tag_name,
            release
                .assets
                .iter()
                .map(|a| a.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    if candidates.next().is_some() {
        return Err(anyhow!(
            "multiple {description} assets in release {}",
            release.tag_name
        ));
    }
    Ok(asset)
}

pub fn install_hlae(directory: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = directory.to_path_buf();
    validate_install_directory(&dir, "advancedfx/advancedfx", force)?;
    recover_install(&dir)?;
    if !force && hlae_installed(&dir) {
        return Ok(dir);
    }
    validate_install_directory(&dir, "advancedfx/advancedfx", true)?;
    install_release(
        "advancedfx/advancedfx",
        &dir,
        hlae_asset,
        hlae_installed,
        log,
    )?;
    Ok(dir)
}

fn ffmpeg_installed(dir: &Path) -> bool {
    super::paths::find_file(dir, "ffmpeg.exe", 4).is_some_and(|exe| {
        exe.parent()
            .is_some_and(|bin| bin.join("ffprobe.exe").is_file())
    })
}

fn ffmpeg_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    unique_asset(
        release,
        |name| name.starts_with("ffmpeg-") && name.ends_with("-win64-gpl.zip"),
        "static win64 GPL FFmpeg ZIP",
    )
}

pub fn install_ffmpeg(directory: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = directory.to_path_buf();
    validate_install_directory(&dir, "BtbN/FFmpeg-Builds", force)?;
    recover_install(&dir)?;
    if !force && ffmpeg_installed(&dir) {
        return Ok(dir);
    }
    validate_install_directory(&dir, "BtbN/FFmpeg-Builds", true)?;
    install_release(
        "BtbN/FFmpeg-Builds",
        &dir,
        ffmpeg_asset,
        ffmpeg_installed,
        log,
    )?;
    Ok(dir)
}

/// BtbN's rolling tag and GitHub's latest release are different publications.
fn release_for_tool(
    repo: &str,
    select: fn(&GithubRelease) -> Result<&GithubAsset>,
    log: Log,
) -> Result<GithubRelease> {
    let sources: &[&str] = if repo == "BtbN/FFmpeg-Builds" {
        &["tags/latest", "latest"]
    } else {
        &["latest"]
    };
    let mut errors = Vec::new();
    for source in sources {
        log.check()?;
        let url = format!("https://api.github.com/repos/{repo}/releases/{source}");
        let fetched: Result<GithubRelease> = (|| {
            let release = transport::agent(log.cancel.clone())
                .get(&url)
                .header("Accept", "application/vnd.github+json")
                .call()?
                .body_mut()
                .read_json()?;
            select(&release)?;
            Ok(release)
        })();
        match fetched {
            Ok(release) => return Ok(release),
            Err(error) => errors.push(format!("{url}: {error:#}")),
        }
    }
    Err(anyhow!(
        "could not resolve a compatible tool download: {}",
        errors.join("; ")
    ))
}

pub fn vrf_exe_name() -> &'static str {
    if cfg!(windows) {
        "Source2Viewer-CLI.exe"
    } else {
        "Source2Viewer-CLI"
    }
}

fn vrf_installed(dir: &Path) -> bool {
    dir.join(vrf_exe_name()).is_file()
}

fn vrf_asset(release: &GithubRelease) -> Result<&GithubAsset> {
    let wanted = if cfg!(windows) {
        "cli-windows-x64.zip"
    } else {
        "cli-linux-x64.zip"
    };
    unique_asset(release, |name| name == wanted, wanted)
}

pub fn install_vrf(directory: &Path, force: bool, log: Log) -> Result<PathBuf> {
    let dir = directory.to_path_buf();
    let exe = dir.join(vrf_exe_name());
    validate_install_directory(&dir, "ValveResourceFormat/ValveResourceFormat", force)?;
    recover_install(&dir)?;
    if !force && vrf_installed(&dir) {
        return Ok(exe);
    }
    validate_install_directory(&dir, "ValveResourceFormat/ValveResourceFormat", true)?;
    install_release(
        "ValveResourceFormat/ValveResourceFormat",
        &dir,
        vrf_asset,
        vrf_installed,
        log,
    )?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&exe, fs::Permissions::from_mode(0o755));
    }
    Ok(exe)
}

/// Release tag recorded by the download button; absent for manual installs.
pub fn installed_release_tag(exe: &Path) -> Option<String> {
    exe.ancestors()
        .skip(1)
        .take(4)
        .find_map(|dir| fs::read(dir.join("install-info.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|info| info["tag"].as_str().map(str::to_owned))
}

/// HLAE is a GUI program with no version flag (unknown arguments open its window),
/// so its version comes from the executable's resource; trailing ".0" trimmed to three parts.
#[cfg(windows)]
fn file_version(exe: &Path) -> Option<String> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileVersionInfoSizeW, GetFileVersionInfoW, VerQueryValueW, VS_FIXEDFILEINFO,
    };
    let path: Vec<u16> = exe.as_os_str().encode_wide().chain([0]).collect();
    let root: Vec<u16> = "\\".encode_utf16().chain([0]).collect();
    let info = unsafe {
        let mut handle = 0;
        let size = GetFileVersionInfoSizeW(path.as_ptr(), &mut handle);
        if size == 0 {
            return None;
        }
        let mut data = vec![0u8; size as usize];
        if GetFileVersionInfoW(path.as_ptr(), 0, size, data.as_mut_ptr().cast()) == 0 {
            return None;
        }
        let mut fixed: *mut core::ffi::c_void = std::ptr::null_mut();
        let mut len = 0u32;
        if VerQueryValueW(data.as_ptr().cast(), root.as_ptr(), &mut fixed, &mut len) == 0
            || (len as usize) < std::mem::size_of::<VS_FIXEDFILEINFO>()
        {
            return None;
        }
        std::ptr::read_unaligned(fixed as *const VS_FIXEDFILEINFO)
    };
    let mut version = format!(
        "{}.{}.{}.{}",
        info.dwFileVersionMS >> 16,
        info.dwFileVersionMS & 0xffff,
        info.dwFileVersionLS >> 16,
        info.dwFileVersionLS & 0xffff
    );
    while version.ends_with(".0") && version.matches('.').count() > 2 {
        version.truncate(version.len() - 2);
    }
    Some(version)
}
#[cfg(not(windows))]
fn file_version(_: &Path) -> Option<String> {
    None
}

/// "ffmpeg version N-126574-g912208af28-20260915 Copyright ..." -> the build id.
fn ffmpeg_banner_version(stdout: &str) -> Option<String> {
    let mut words = stdout.lines().next()?.split_whitespace();
    (words.next()? == "ffmpeg" && words.next()? == "version")
        .then(|| words.next())
        .flatten()
        .map(str::to_owned)
}

/// "Version: 20.0.6980+a06886f7d06049052d32a7381ec05523064a2ca0" -> "20.0.6980"
fn vrf_banner_version(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .find_map(|line| line.strip_prefix("Version:"))
        .map(|version| version.trim().split('+').next().unwrap_or("").to_owned())
        .filter(|version| !version.is_empty())
}

fn probe_stdout(exe: &Path, flag: &str) -> Option<String> {
    let (status, stdout, _) = super::process::probe_output(
        std::process::Command::new(exe).arg(flag),
        Duration::from_secs(10),
    )
    .ok()?;
    status.is_some_and(|s| s.success()).then_some(stdout)
}

fn installed_version(tool: super::SetupTool, exe: &Path) -> Option<String> {
    match tool {
        super::SetupTool::Hlae => file_version(exe),
        super::SetupTool::Ffmpeg => ffmpeg_banner_version(&probe_stdout(exe, "-version")?),
        super::SetupTool::Vrf => vrf_banner_version(&probe_stdout(exe, "--version")?),
    }
}

/// Installed and online versions side by side; whether to update is the user's call.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolUpdate {
    pub installed: Option<String>,
    pub latest: String,
}

/// GitHub's website is not rate limited like its API (60 requests per hour per address):
/// releases/latest redirects to the latest tag, and the Atom feed names the rolling BtbN build.
fn latest_version(tool: super::SetupTool) -> Result<String> {
    let repo = match tool {
        super::SetupTool::Hlae => "advancedfx/advancedfx",
        super::SetupTool::Ffmpeg => "BtbN/FFmpeg-Builds",
        super::SetupTool::Vrf => "ValveResourceFormat/ValveResourceFormat",
    };
    if tool == super::SetupTool::Ffmpeg {
        let url = format!("https://github.com/{repo}/releases.atom");
        let feed = agent()
            .get(&url)
            .config()
            .timeout_global(Some(Duration::from_secs(10)))
            .build()
            .call()
            .with_context(|| format!("GET {url}"))?
            .body_mut()
            .read_to_string()?;
        return rolling_release_title(&feed)
            .ok_or_else(|| anyhow!("{url}: no release named for tag \"latest\""));
    }
    let url = format!("https://github.com/{repo}/releases/latest");
    let response = agent()
        .get(&url)
        .config()
        .timeout_global(Some(Duration::from_secs(10)))
        .max_redirects(0)
        .http_status_as_error(false)
        .build()
        .call()
        .with_context(|| format!("GET {url}"))?;
    response
        .headers()
        .get("Location")
        .and_then(|value| value.to_str().ok())
        .and_then(|location| location.rsplit_once("/releases/tag/"))
        .map(|(_, tag)| tag.to_owned())
        .ok_or_else(|| anyhow!("{url}: http status {} without a release tag", response.status()))
}

/// Title of the feed entry linking to releases/tag/latest.
fn rolling_release_title(feed: &str) -> Option<String> {
    let entry = &feed[feed.find("/releases/tag/latest\"")?..];
    let title = &entry[entry.find("<title>")? + "<title>".len()..];
    Some(title[..title.find("</title>")?].trim().to_owned())
}

pub fn check_update(tool: super::SetupTool, exe: &Path) -> Result<ToolUpdate> {
    Ok(ToolUpdate {
        latest: latest_version(tool)?,
        installed: installed_version(tool, exe),
    })
}

/// Tells HLAE where ffmpeg lives: <hlaeDir>/ffmpeg/ffmpeg.ini with [Ffmpeg] Path=…
pub fn register_ffmpeg_with_hlae(hlae_exe: &Path, ffmpeg_exe: &Path) -> Result<()> {
    let ini_dir = hlae_exe.parent().unwrap().join("ffmpeg");
    fs::create_dir_all(&ini_dir)?;
    fs::write(
        ini_dir.join("ffmpeg.ini"),
        format!("[Ffmpeg]\nPath={}\n", ffmpeg_exe.display()),
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn cancelling_download_stops_writing_before_next_chunk() {
        let cancel = Arc::new(AtomicBool::new(false));
        let mut output = Vec::new();
        let result = copy_download(
            &mut std::io::Cursor::new(vec![1; 200_000]),
            &mut output,
            Some(200_000),
            &mut Progress {
                cancel: &cancel,
                report: &mut |_| cancel.store(true, Ordering::Relaxed),
            },
        );
        assert!(result.is_err());
        assert_eq!(output.len(), 1 << 16);
    }
    #[test]
    fn download_reports_small_and_unknown_size_transfers_and_rejects_truncation() {
        for total in [Some(10_000_000), Some(100_000), None] {
            let mut reader = std::io::Cursor::new(vec![42; 100_000]);
            let mut output = Vec::new();
            let mut logs = Vec::new();
            let result = super::copy_download(
                &mut reader,
                &mut output,
                total,
                &mut Progress {
                    cancel: &Arc::new(AtomicBool::new(false)),
                    report: &mut |line| logs.push(line),
                },
            );
            assert_eq!(output.len(), 100_000);
            assert!(
                !logs.is_empty(),
                "progress must appear below 10% and without Content-Length"
            );
            assert_eq!(
                result.is_err(),
                total == Some(10_000_000),
                "truncated bodies must fail"
            );
        }
    }

    use super::*;

    fn release(names: &[&str]) -> GithubRelease {
        GithubRelease {
            tag_name: "autobuild".into(),
            assets: names
                .iter()
                .map(|name| GithubAsset {
                    name: (*name).into(),
                    browser_download_url: format!("https://example.invalid/{name}"),
                })
                .collect(),
        }
    }

    #[test]
    fn tool_version_banners_are_parsed() {
        let banner =
            "ffmpeg version N-126574-g912208af28-20260915 Copyright (c) 2000-2026\nbuilt with gcc";
        assert_eq!(
            ffmpeg_banner_version(banner).as_deref(),
            Some("N-126574-g912208af28-20260915")
        );
        assert!(ffmpeg_banner_version("garbage").is_none());
        assert_eq!(
            vrf_banner_version("Version: 20.0.6980+a06886f7d0604905\nOS: Microsoft Windows").as_deref(),
            Some("20.0.6980")
        );
        assert!(vrf_banner_version("OS: Microsoft Windows").is_none());
        let feed = r#"<feed><title>Release notes</title><entry><id>1</id><link href="https://github.com/BtbN/FFmpeg-Builds/releases/tag/latest"/><title>Latest Auto-Build (2026-09-23 14:55)</title></entry><entry><link href="https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-09-23-14-55"/><title>Auto-Build 2026-09-23 14:55</title></entry></feed>"#;
        assert_eq!(
            rolling_release_title(feed).as_deref(),
            Some("Latest Auto-Build (2026-09-23 14:55)")
        );
        assert!(rolling_release_title("<feed></feed>").is_none());

        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("ffmpeg-N-1-win64-gpl/bin");
        fs::create_dir_all(&bin).unwrap();
        assert!(installed_release_tag(&bin.join("ffmpeg.exe")).is_none());
        fs::write(dir.path().join("install-info.json"), r#"{"tag":"latest","url":"x"}"#).unwrap();
        assert_eq!(
            installed_release_tag(&bin.join("ffmpeg.exe")).as_deref(),
            Some("latest")
        );
    }

    #[test]
    #[ignore = "requires DEMODESK_TEST_TOOLS_DIR pointing to installed tools and network access"]
    fn installed_tool_update_checks() {
        let root = PathBuf::from(
            std::env::var_os("DEMODESK_TEST_TOOLS_DIR").expect("set DEMODESK_TEST_TOOLS_DIR"),
        );
        let paths = super::super::paths::resolve_tool_paths(&root, &Default::default());
        for tool in [
            super::super::SetupTool::Hlae,
            super::super::SetupTool::Ffmpeg,
            super::super::SetupTool::Vrf,
        ] {
            let Some(exe) = super::super::diagnostics::executable(&paths, tool) else {
                continue;
            };
            let update = check_update(tool, exe).unwrap();
            println!("{tool:?}: {update:?}");
            assert!(update.installed.is_some(), "{tool:?} version not detected");
        }
    }

    #[test]
    fn ffmpeg_selection_accepts_versioned_names_and_rejects_other_builds() {
        let name = "ffmpeg-N-126475-g35b7df64a0-win64-gpl.zip";
        let current = release(&[
            "ffmpeg-N-126475-win64-gpl-shared.zip",
            "ffmpeg-N-126475-win64-lgpl.zip",
            "ffmpeg-N-126475-winarm64-gpl.zip",
            "ffmpeg-n9.0.1-win64-gpl-9.0.zip",
            name,
        ]);
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
        assert_eq!(
            hlae_asset(&release(&[
                "HLAE_setup.exe",
                "hlae_2_191_1.zip.asc",
                "hlae_2_191_1.zip"
            ]))
            .unwrap()
            .name,
            "hlae_2_191_1.zip"
        );
        assert!(hlae_asset(&release(&["source.zip", "HLAE_setup.exe"])).is_err());
        let platform = if cfg!(windows) {
            "cli-windows-x64.zip"
        } else {
            "cli-linux-x64.zip"
        };
        assert_eq!(
            vrf_asset(&release(&[
                "cli-linux-arm64.zip",
                "gui-windows-x64.zip",
                platform
            ]))
            .unwrap()
            .name,
            platform
        );
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
        fs::rename(&dir, previous_directory(&dir)).unwrap();
        recover_install(&dir).unwrap();
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        let staged = root.path().join("new");
        fs::create_dir(&staged).unwrap();
        fs::write(staged.join("new"), b"ready").unwrap();
        publish_install(&dir, &staged).unwrap();
        assert!(dir.join("new").is_file());
        assert!(!dir.join("old").exists());
        assert!(!previous_directory(&dir).exists());
    }

    #[test]
    fn custom_directory_install_preserves_unrelated_folders_and_replaces_owned_tools() {
        use std::io::Write;
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("chosen.previous");
        fs::create_dir(&dir).unwrap();
        let work = root.path().join("stage");
        fs::create_dir(&work).unwrap();
        let mut zip = zip::ZipWriter::new(fs::File::create(work.join("download.zip")).unwrap());
        for file in ["HLAE.exe", "x64/AfxHookSource2.dll"] {
            zip.start_file(file, zip::write::SimpleFileOptions::default())
                .unwrap();
            zip.write_all(b"synthetic binary").unwrap();
        }
        zip.finish().unwrap();
        let url = "https://github.com/advancedfx/advancedfx/releases/download/test/hlae.zip";
        fs::write(dir.join("keep.txt"), b"unrelated").unwrap();
        assert!(install_archive(
            &dir,
            &work,
            "test",
            url,
            hlae_installed,
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(dir.join("keep.txt")).unwrap(), b"unrelated");
        fs::remove_file(dir.join("keep.txt")).unwrap();
        install_archive(
            &dir,
            &work,
            "test",
            url,
            hlae_installed,
            &AtomicBool::new(false),
        )
        .unwrap();
        assert!(hlae_installed(&dir));
        validate_install_directory(&dir, "advancedfx/advancedfx", true).unwrap();
        assert!(validate_install_directory(&dir, "BtbN/FFmpeg-Builds", true).is_err());
        assert_ne!(previous_directory(&dir), dir);
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
        let cancelled = install_archive(
            &dir,
            &work,
            "test",
            "test",
            hlae_installed,
            &AtomicBool::new(true),
        )
        .unwrap_err();
        assert!(cancelled.to_string().contains("cancelled"));
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        assert!(install_archive(
            &dir,
            &work,
            "test",
            "test",
            hlae_installed,
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
        let mut zip = zip::ZipWriter::new(fs::File::create(work.join("download.zip")).unwrap());
        zip.start_file("README.txt", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"missing binaries").unwrap();
        zip.finish().unwrap();
        assert!(install_archive(
            &dir,
            &work,
            "test",
            "test",
            hlae_installed,
            &AtomicBool::new(false)
        )
        .is_err());
        assert_eq!(fs::read(dir.join("old")).unwrap(), b"working");
    }
}
