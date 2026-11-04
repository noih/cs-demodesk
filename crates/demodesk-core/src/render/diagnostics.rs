pub mod encoding;

use super::{paths::ToolPaths, process::probe_output, SetupTool};
use serde::Serialize;
use std::{
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint(Vec<(PathBuf, Option<(u64, SystemTime)>)>);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolCheck {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub encoding: Option<encoding::Report>,
    pub ok: bool,
    pub path: Option<PathBuf>,
    pub installed_release: Option<String>,
    pub actual_version: Option<String>,
    pub checked_at: String,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
    #[serde(skip)]
    pub fingerprint: Fingerprint,
}

pub fn executable(paths: &ToolPaths, tool: SetupTool) -> Option<&PathBuf> {
    match tool {
        SetupTool::Hlae => paths.hlae_exe.as_ref(),
        SetupTool::Ffmpeg => paths.ffmpeg_exe.as_ref(),
        SetupTool::Vrf => paths.vrf_exe.as_ref(),
    }
}

pub fn fingerprint(paths: &ToolPaths, tool: SetupTool) -> Fingerprint {
    let mut files: Vec<_> = executable(paths, tool).cloned().into_iter().collect();
    if let Some(exe) = files.first() {
        if tool == SetupTool::Hlae {
            files.push(exe.parent().unwrap().join("x64/AfxHookSource2.dll"));
        } else if tool == SetupTool::Ffmpeg {
            let bin = exe.parent().unwrap().to_path_buf();
            files.push(exe.with_file_name(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            }));
            // Shared builds need their DLLs; changes must invalidate the startup check.
            let mut dlls: Vec<_> = std::fs::read_dir(bin)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| {
                    path.extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("dll"))
                })
                .collect();
            dlls.sort();
            files.extend(dlls);
        }
    }
    Fingerprint(
        files
            .into_iter()
            .map(|path| {
                let meta = std::fs::metadata(&path)
                    .ok()
                    .filter(|m| m.is_file())
                    .and_then(|m| Some((m.len(), m.modified().ok()?)));
                (path, meta)
            })
            .collect(),
    )
}

pub fn tool_report(
    paths: &ToolPaths,
    tool: SetupTool,
    cached: Option<&ToolCheck>,
) -> serde_json::Value {
    let path = executable(paths, tool);
    let mut report = serde_json::json!({ "path": path, "cache": "missing", "lastCheck": null });
    let Some(check) = cached else {
        return report;
    };
    report["lastCheck"] = serde_json::json!({
        "ok": check.ok, "path": check.path, "installedRelease": check.installed_release,
        "actualVersion": check.actual_version, "checkedAt": check.checked_at, "exitCode": check.exit_code,
        "timedOut": check.timed_out, "error": check.error,
    });
    if let Some(encoding) = &check.encoding {
        report["lastCheck"]["encoding"] = serde_json::to_value(encoding).unwrap_or_default();
    }
    if !check.ok {
        for (name, output) in [("stdout", &check.stdout), ("stderr", &check.stderr)] {
            if !output.is_empty() {
                report["lastCheck"][name] = output.as_str().into();
            }
        }
    }
    if check.fingerprint == fingerprint(paths, tool) {
        report["cache"] = "valid".into();
    } else {
        report["cache"] = "stale".into();
        report["cacheReason"] = if path.is_none() {
            "Executable is missing or no longer resolves"
        } else if check.path.as_ref() != path {
            "Executable path changed"
        } else {
            "Required files changed or are missing"
        }
        .into();
    }
    report
}

pub fn check(paths: &ToolPaths, tool: SetupTool) -> ToolCheck {
    let mut result = ToolCheck {
        encoding: None,
        ok: false,
        path: executable(paths, tool).cloned(),
        installed_release: None,
        actual_version: None,
        checked_at: chrono::Utc::now().to_rfc3339(),
        exit_code: None,
        timed_out: false,
        stdout: String::new(),
        stderr: String::new(),
        error: None,
        fingerprint: fingerprint(paths, tool),
    };
    let Some(path) = &result.path else {
        result.error = Some("Tool executable not found".into());
        return result;
    };
    result.installed_release = super::setup::installed_release_tag(path);
    if tool == SetupTool::Hlae {
        result.actual_version = super::setup::file_version(path);
    }
    if let Some((file, _)) = result
        .fingerprint
        .0
        .iter()
        .find(|(_, metadata)| metadata.is_none())
    {
        result.error = Some(format!(
            "Required file is missing or inaccessible: {}",
            file.display()
        ));
        return result;
    }
    let mut command = Command::new(path);
    match tool {
        SetupTool::Hlae => {
            command.args(["-customLoader", "-noGui", "-noConfig"]);
        }
        SetupTool::Ffmpeg => {
            command.arg("-version");
        }
        SetupTool::Vrf => {
            command.arg("--version");
        }
    }
    match probe_output(&mut command, Duration::from_secs(10)) {
        Ok((status, stdout, stderr)) => {
            result.exit_code = status.and_then(|s| s.code());
            result.timed_out = status.is_none();
            result.ok = status.is_some_and(|s| s.success());
            result.actual_version = match tool {
                SetupTool::Hlae => result.actual_version,
                SetupTool::Ffmpeg => super::setup::ffmpeg_banner_version(&stdout)
                    .or_else(|| super::setup::ffmpeg_banner_version(&stderr)),
                SetupTool::Vrf => super::setup::vrf_banner_version(&stdout)
                    .or_else(|| super::setup::vrf_banner_version(&stderr)),
            };
            result.stdout = stdout;
            result.stderr = stderr;
            if !result.ok {
                result.error = Some(if result.timed_out {
                    "Startup check timed out after 10 seconds".into()
                } else {
                    format!("Tool exited with code {:?}", result.exit_code)
                });
            }
        }
        Err(error) => result.error = Some(format!("Cannot start tool: {error}")),
    }
    result
}

/// Read versions without starting HLAE or changing the installation-check cache.
pub fn current_version(paths: &ToolPaths, tool: SetupTool) -> serde_json::Value {
    let result = (|| -> Result<String, String> {
        let path = executable(paths, tool).ok_or("Tool executable not found")?;
        if tool == SetupTool::Hlae {
            return super::setup::file_version(path)
                .ok_or("Executable version metadata unavailable".into());
        }
        let flag = if tool == SetupTool::Ffmpeg {
            "-version"
        } else {
            "--version"
        };
        let (status, stdout, stderr) =
            probe_output(Command::new(path).arg(flag), Duration::from_secs(10))
                .map_err(|e| format!("Cannot read tool version: {e}"))?;
        let Some(status) = status else {
            return Err("Version query timed out after 10 seconds".into());
        };
        if !status.success() {
            return Err(format!(
                "Version query exited with {:?}: {stderr}",
                status.code()
            ));
        }
        let parse = if tool == SetupTool::Ffmpeg {
            super::setup::ffmpeg_banner_version
        } else {
            super::setup::vrf_banner_version
        };
        parse(&stdout)
            .or_else(|| parse(&stderr))
            .ok_or("Version output not recognized".into())
    })();
    match result {
        Ok(version) => {
            serde_json::json!({"actualVersion": version, "checkedAt": crate::store::now()})
        }
        Err(error) => {
            serde_json::json!({"actualVersion": null, "error": error, "checkedAt": crate::store::now()})
        }
    }
}

/// Sanitize only the shareable report; never mutate paths used by the application.
pub fn redact_paths(value: &mut serde_json::Value) {
    let mut roots = Vec::new();
    for (path, label) in [
        (value["paths"]["toolsDir"].as_str(), "[TOOLS]"),
        (value["paths"]["cs2Dir"].as_str(), "[CS2]"),
        (value["paths"]["steamDir"].as_str(), "[STEAM]"),
        (value["dataDirectory"].as_str(), "[DATA]"),
        (value["pendingDataDirectory"].as_str(), "[PENDING_DATA]"),
    ] {
        if let Some(path) = path.filter(|p| !p.is_empty()) {
            roots.push((path.replace('\\', "/"), label));
        }
    }
    for (key, label) in [
        ("ffmpegExe", "[FFMPEG]"),
        ("hlaeExe", "[HLAE]"),
        ("vrfExe", "[VRF]"),
    ] {
        if let Some(path) = value["paths"][key].as_str() {
            let normalized = path.replace('\\', "/");
            // Bundled tools retain their layout under [TOOLS]; custom installations get their own root.
            if !roots.iter().any(|(root, _)| {
                normalized
                    .to_lowercase()
                    .starts_with(&(root.to_lowercase() + "/"))
            }) {
                if let Some((parent, _)) = normalized.rsplit_once('/') {
                    roots.push((parent.to_owned(), label));
                }
            }
        }
    }
    roots.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
    redact_value(value, &roots);
}

fn redact_value(value: &mut serde_json::Value, roots: &[(String, &str)]) {
    match value {
        serde_json::Value::String(text) => *text = redact_path_text(text, roots),
        serde_json::Value::Array(values) => values.iter_mut().for_each(|v| redact_value(v, roots)),
        serde_json::Value::Object(values) => {
            values.values_mut().for_each(|v| redact_value(v, roots))
        }
        _ => {}
    }
}

fn redact_path_text(text: &str, roots: &[(String, &str)]) -> String {
    let mut output = String::new();
    let mut chars = text.char_indices().peekable();
    let mut previous = None;
    while let Some((index, ch)) = chars.next() {
        let rest = &text[index..];
        if let Some((root, label)) = roots.iter().find(|(root, _)| {
            rest.get(..root.len())
                .is_some_and(|prefix| prefix.replace('\\', "/").eq_ignore_ascii_case(root))
                && rest[root.len()..].chars().next().is_none_or(|c| {
                    c == '/' || c == '\\' || c.is_whitespace() || "\"':,)]".contains(c)
                })
        }) {
            output.push_str(label);
            while chars
                .peek()
                .is_some_and(|(offset, _)| *offset < index + root.len())
            {
                chars.next();
            }
            previous = Some(']');
            continue;
        }
        let boundary = previous.is_none_or(|c: char| c.is_whitespace() || "\"'=([{,".contains(c));
        let drive = previous.is_none_or(|c: char| !c.is_alphanumeric())
            && ch.is_ascii_alphabetic()
            && (rest.as_bytes().get(1) == Some(&b':'))
            && matches!(rest.as_bytes().get(2), Some(b'/' | b'\\'));
        let network = rest.starts_with("\\\\");
        let unix = boundary && ch == '/' && rest.len() > 1;
        if drive || network || unix || rest.starts_with("file://") {
            output.push_str("[PATH]");
            // Unquoted paths may contain spaces: redact through the next delimiter.
            while let Some(&(next_index, next)) = chars.peek() {
                let closing_quote = next == '\''
                    && previous == Some('\'')
                    && text[next_index + 1..]
                        .chars()
                        .next()
                        .is_none_or(|c| !c.is_alphanumeric());
                if "\"\n\r<>|".contains(next)
                    || closing_quote
                    || (next == ':' && text[next_index + 1..].starts_with(' '))
                {
                    break;
                }
                chars.next();
            }
        } else {
            output.push(ch);
        }
        previous = Some(ch);
    }
    output
}

pub fn environment() -> serde_json::Value {
    let mut result = serde_json::json!({
        "appVersion": env!("CARGO_PKG_VERSION"), "os": std::env::consts::OS, "architecture": std::env::consts::ARCH,
        "packaged": crate::updates::is_packaged().ok(),
    });
    #[cfg(windows)]
    {
        use winreg::{
            enums::{HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_64KEY},
            RegKey,
        };
        let machine = RegKey::predef(HKEY_LOCAL_MACHINE);
        if let Ok(key) = machine.open_subkey_with_flags(
            "SOFTWARE\\Microsoft\\Windows NT\\CurrentVersion",
            KEY_READ | KEY_WOW64_64KEY,
        ) {
            result["windowsBuild"] = key.get_value::<String, _>("CurrentBuildNumber").ok().into();
            result["windowsRevision"] = key.get_value::<u32, _>("UBR").ok().into();
        }
        result["dotNetFrameworkRelease"] = machine
            .open_subkey_with_flags(
                "SOFTWARE\\Microsoft\\NET Framework Setup\\NDP\\v4\\Full",
                KEY_READ | KEY_WOW64_64KEY,
            )
            .ok()
            .and_then(|key| key.get_value::<u32, _>("Release").ok())
            .into();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redaction_preserves_tool_layout_and_following_log_details() {
        let mut report = serde_json::json!({
            "dataDirectory": "D:/Private Folder/app",
            "paths": {"toolsDir": "D:/Private Folder/app/tools", "ffmpegExe": "D:/Private Folder/app/tools/ffmpeg/bin/ffmpeg.exe", "hlaeExe": "E:/Other Private/HLAE.exe"},
            "log": "Cannot open D:\\Private Folder\\app\\tools\\ffmpeg\\bin\\avcodec.dll: access denied; attempt 2",
            "outside": "D:/Private Folder/app-other/private.dem"
        });
        redact_paths(&mut report);
        assert_eq!(report["dataDirectory"], "[DATA]");
        assert_eq!(
            report["paths"]["ffmpegExe"],
            "[TOOLS]/ffmpeg/bin/ffmpeg.exe"
        );
        assert_eq!(report["paths"]["hlaeExe"], "[HLAE]/HLAE.exe");
        assert_eq!(
            report["log"],
            "Cannot open [TOOLS]\\ffmpeg\\bin\\avcodec.dll: access denied; attempt 2"
        );
        assert_eq!(report["outside"], "[PATH]");
        assert!(!report.to_string().contains("Private"));
    }

    #[test]
    fn missing_version_has_an_explicit_error() {
        let report = current_version(&ToolPaths::default(), SetupTool::Ffmpeg);
        assert!(report["actualVersion"].is_null());
        assert_eq!(report["error"], "Tool executable not found");
    }

    #[test]
    fn shared_report_redacts_paths_in_nested_errors_and_logs() {
        let mut report = serde_json::json!({
            "path": "C:\\Private Folder\\Videos\\clip.mp4",
            "pendingDataDirectory": "D:/Private Folder/data",
            "nested": [{"error": "Cannot open 'E:\\Private Folder\\clip.mp4': permission denied"}],
            "log": ["input \\\\private-server\\share\\clip.mp4", "file /home/private/video.mp4", "codec libx265; attempt 2; 300000 bytes"],
            "url": "https://example.com/releases",
            "uri": "file:///home/private/video.mp4",
            "apostrophe": "C:\\Private Folder\\O'Brien\\clip.mp4",
            "version": "N-128352-gec887b13a8"
        });
        redact_paths(&mut report);
        let text = report.to_string();
        for secret in ["Private", "private", "clip.mp4", "video.mp4"] {
            assert!(!text.contains(secret), "{text}");
        }
        assert_eq!(
            report["nested"][0]["error"],
            "Cannot open '[PATH]': permission denied"
        );
        assert_eq!(report["url"], "https://example.com/releases");
        assert_eq!(report["uri"], "[PATH]");
        assert_eq!(report["apostrophe"], "[PATH]");
        assert_eq!(report["log"][2], "codec libx265; attempt 2; 300000 bytes");
        assert_eq!(report["version"], "N-128352-gec887b13a8");
    }

    #[test]
    fn shared_ffmpeg_dll_changes_invalidate_cached_check() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("ffmpeg.exe");
        std::fs::write(&exe, b"ffmpeg").unwrap();
        std::fs::write(dir.path().join("ffprobe.exe"), b"ffprobe").unwrap();
        let paths = ToolPaths {
            ffmpeg_exe: Some(exe),
            ..Default::default()
        };
        let static_build = fingerprint(&paths, SetupTool::Ffmpeg);
        let dll = dir.path().join("avcodec-63.dll");
        std::fs::write(&dll, b"shared library").unwrap();
        let shared_build = fingerprint(&paths, SetupTool::Ffmpeg);
        assert_ne!(static_build, shared_build);
        std::fs::write(&dll, b"updated library").unwrap();
        assert_ne!(shared_build, fingerprint(&paths, SetupTool::Ffmpeg));
        std::fs::remove_file(&dll).unwrap();
        assert_eq!(static_build, fingerprint(&paths, SetupTool::Ffmpeg));
    }

    #[test]
    fn report_explains_cache_state_and_only_includes_useful_check_output() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("HLAE.exe");
        let companion = temp.path().join("x64/AfxHookSource2.dll");
        std::fs::create_dir(companion.parent().unwrap()).unwrap();
        std::fs::write(&exe, b"report must not execute this").unwrap();
        std::fs::write(&companion, []).unwrap();
        let mut paths = ToolPaths {
            hlae_exe: Some(exe.clone()),
            ..Default::default()
        };
        assert_eq!(
            tool_report(&paths, SetupTool::Hlae, None),
            serde_json::json!({ "path": exe, "cache": "missing", "lastCheck": null })
        );
        let mut cached = ToolCheck {
            encoding: None,
            ok: true,
            path: Some(exe.clone()),
            installed_release: Some("v1".into()),
            actual_version: Some("1.2.3".into()),
            checked_at: "2026-09-17T00:00:00Z".into(),
            exit_code: Some(0),
            timed_out: false,
            stdout: "version banner".into(),
            stderr: "diagnostic output".into(),
            error: None,
            fingerprint: fingerprint(&paths, SetupTool::Hlae),
        };
        let valid = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(valid["cache"], "valid");
        assert_eq!(valid["lastCheck"]["actualVersion"], "1.2.3");
        assert!(valid.get("cacheReason").is_none());
        assert!(valid["lastCheck"].get("stdout").is_none());
        assert!(valid["lastCheck"].get("stderr").is_none());
        assert!(valid["lastCheck"].get("fingerprint").is_none());

        cached.encoding = Some(encoding::check(None));
        let advisory = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(advisory["lastCheck"]["ok"], true);
        assert_eq!(advisory["lastCheck"]["encoding"]["results"].as_array().unwrap().len(), 6);
        assert_eq!(advisory["lastCheck"]["encoding"]["results"][0]["status"], "skipped");
        cached.encoding = None;

        cached.ok = false;
        cached.exit_code = Some(7);
        cached.error = Some("Tool exited with code 7".into());
        let failed = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(failed["cache"], "valid");
        assert_eq!(failed["lastCheck"], serde_json::to_value(&cached).unwrap());
        cached.stdout.clear();
        cached.stderr.clear();
        let empty_output = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert!(empty_output["lastCheck"].get("stdout").is_none());
        assert!(empty_output["lastCheck"].get("stderr").is_none());

        std::fs::write(&companion, b"changed").unwrap();
        let stale = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(stale["cache"], "stale");
        assert_eq!(
            stale["cacheReason"],
            "Required files changed or are missing"
        );
        assert_eq!(stale["lastCheck"], empty_output["lastCheck"]);
        std::fs::remove_file(companion).unwrap();
        assert_eq!(
            tool_report(&paths, SetupTool::Hlae, Some(&cached))["cache"],
            "stale"
        );
        paths.hlae_exe = Some(temp.path().join("new/HLAE.exe"));
        let moved = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(moved["path"], serde_json::json!(paths.hlae_exe));
        assert_eq!(moved["cache"], "stale");
        assert_eq!(moved["cacheReason"], "Executable path changed");
        assert_eq!(moved["lastCheck"]["path"], serde_json::json!(exe));
        paths.hlae_exe = None;
        let missing = tool_report(&paths, SetupTool::Hlae, Some(&cached));
        assert_eq!(missing["cache"], "stale");
        assert_eq!(
            missing["cacheReason"],
            "Executable is missing or no longer resolves"
        );
        assert_eq!(missing["lastCheck"], empty_output["lastCheck"]);
    }

    #[test]
    #[ignore = "requires DEMODESK_TEST_TOOLS_DIR pointing to installed tools"]
    fn installed_tools_startup() {
        let root = PathBuf::from(
            std::env::var_os("DEMODESK_TEST_TOOLS_DIR").expect("set DEMODESK_TEST_TOOLS_DIR"),
        );
        let paths = super::super::paths::resolve_tool_paths(&root, &Default::default());
        let mut tested = 0;
        for tool in [SetupTool::Hlae, SetupTool::Ffmpeg, SetupTool::Vrf] {
            if executable(&paths, tool).is_none() {
                continue;
            }
            let result = check(&paths, tool);
            println!("{tool:?}: {}", serde_json::to_string(&result).unwrap());
            assert!(result.ok, "{tool:?} startup failed");
            let version = current_version(&paths, tool);
            assert_eq!(
                version["actualVersion"].as_str(),
                result.actual_version.as_deref()
            );
            assert!(version.get("error").is_none(), "{version}");
            assert!(
                result.actual_version.is_some(),
                "{tool:?} actual version missing"
            );
            tested += 1;
        }
        assert!(tested > 0, "No installed tools found");
    }

    #[test]
    fn probe_fixture() {
        let Ok(mode) = std::env::var("DEMODESK_STARTUP_PROBE_TEST") else {
            return;
        };
        println!("fixture version 1");
        eprintln!("fixture diagnostic");
        match mode.as_str() {
            "fail" => std::process::exit(7),
            "hang" => std::thread::sleep(Duration::from_secs(30)),
            "large" => print!("{}", "x".repeat(40_000)),
            _ => {}
        }
    }

    #[test]
    fn probe_captures_failure_limits_output_and_terminates_timeouts() {
        for mode in ["ok", "fail", "hang", "large"] {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "render::diagnostics::tests::probe_fixture",
                    "--nocapture",
                ])
                .env("DEMODESK_STARTUP_PROBE_TEST", mode);
            let started = std::time::Instant::now();
            let (status, stdout, stderr) =
                probe_output(&mut command, Duration::from_millis(500)).unwrap();
            assert!(stdout.contains("fixture version 1"));
            assert!(stderr.contains("fixture diagnostic"));
            assert!(stdout.len() <= 16 * 1024);
            match mode {
                "fail" => assert_eq!(status.unwrap().code(), Some(7)),
                "hang" => {
                    assert!(status.is_none());
                    assert!(started.elapsed() < Duration::from_secs(3));
                }
                _ => assert!(status.unwrap().success()),
            }
        }
    }

    #[test]
    fn missing_dependencies_bad_executables_and_removed_files_are_not_ready() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("HLAE.exe");
        std::fs::write(&exe, b"invalid executable").unwrap();
        let paths = ToolPaths {
            hlae_exe: Some(exe.clone()),
            ..Default::default()
        };
        let missing = check(&paths, SetupTool::Hlae);
        assert!(!missing.ok);
        assert!(missing.error.unwrap().contains("AfxHookSource2.dll"));
        std::fs::create_dir(temp.path().join("x64")).unwrap();
        std::fs::write(temp.path().join("x64/AfxHookSource2.dll"), []).unwrap();
        let invalid = check(&paths, SetupTool::Hlae);
        assert!(!invalid.ok);
        assert!(invalid.error.unwrap().contains("Cannot start tool"));
        std::fs::remove_file(exe).unwrap();
        assert_ne!(invalid.fingerprint, fingerprint(&paths, SetupTool::Hlae));
        assert!(!check(&paths, SetupTool::Hlae).ok);
    }
}
