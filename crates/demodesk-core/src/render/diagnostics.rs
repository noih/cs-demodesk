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
    pub ok: bool,
    pub path: Option<PathBuf>,
    pub installed_release: Option<String>,
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
            files.push(exe.with_file_name(if cfg!(windows) {
                "ffprobe.exe"
            } else {
                "ffprobe"
            }));
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
        "checkedAt": check.checked_at, "exitCode": check.exit_code,
        "timedOut": check.timed_out, "error": check.error,
    });
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
        ok: false,
        path: executable(paths, tool).cloned(),
        installed_release: None,
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
    result.installed_release = path
        .parent()
        .and_then(|dir| std::fs::read(dir.join("install-info.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|info| info["tag"].as_str().map(str::to_owned));
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
            ok: true,
            path: Some(exe.clone()),
            installed_release: Some("v1".into()),
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
        assert!(valid.get("cacheReason").is_none());
        assert!(valid["lastCheck"].get("stdout").is_none());
        assert!(valid["lastCheck"].get("stderr").is_none());
        assert!(valid["lastCheck"].get("fingerprint").is_none());

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
