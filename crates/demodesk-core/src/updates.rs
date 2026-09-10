use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum Update {
    Packaged,
    Current,
    Available { version: String },
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    draft: bool,
    prerelease: bool,
}

#[cfg(windows)]
fn is_packaged() -> Result<bool> {
    use windows_sys::Win32::Foundation::{APPMODEL_ERROR_NO_PACKAGE, ERROR_INSUFFICIENT_BUFFER};
    use windows_sys::Win32::Storage::Packaging::Appx::GetCurrentPackageFullName;
    let mut length = 0;
    // The API supports a null output buffer to query the required length.
    let result = unsafe { GetCurrentPackageFullName(&mut length, std::ptr::null_mut()) };
    match result {
        APPMODEL_ERROR_NO_PACKAGE => Ok(false),
        ERROR_INSUFFICIENT_BUFFER => Ok(true),
        error => bail!("Cannot determine package identity: {error}"),
    }
}

#[cfg(not(windows))]
fn is_packaged() -> Result<bool> {
    Ok(false)
}

fn newer_release(current: &str, release: Release) -> Result<Update> {
    let latest = semver::Version::parse(
        release
            .tag_name
            .strip_prefix('v')
            .unwrap_or(&release.tag_name),
    )?;
    let current = semver::Version::parse(current)?;
    if !release.draft
        && !release.prerelease
        && latest.pre.is_empty()
        && latest.cmp_precedence(&current).is_gt()
    {
        Ok(Update::Available {
            version: latest.to_string(),
        })
    } else {
        Ok(Update::Current)
    }
}

pub fn check() -> Result<Update> {
    // Sideloaded packages also use the packaged route; never replace their EXE.
    if is_packaged()? {
        return Ok(Update::Packaged);
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    let release: Release = agent
        .get("https://api.github.com/repos/noih/cs-demodesk/releases/latest")
        .header("User-Agent", "CS-DemoDesk")
        .header("Accept", "application/vnd.github+json")
        .call()?
        .body_mut()
        .read_json()?;
    newer_release(env!("CARGO_PKG_VERSION"), release)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_versions_and_ignores_unpublished_versions() {
        for (tag, draft, prerelease, expected) in [
            ("v1.0.10", false, false, true),
            ("v1.0.9", false, false, false),
            ("v1.0.9+build.2", false, false, false),
            ("v1.0.8", false, false, false),
            ("v2.0.0", true, false, false),
            ("v2.0.0", false, true, false),
            ("v2.0.0-beta.1", false, false, false),
        ] {
            let result = newer_release(
                "1.0.9",
                Release {
                    tag_name: tag.into(),
                    draft,
                    prerelease,
                },
            )
            .unwrap();
            assert_eq!(
                matches!(result, Update::Available { .. }),
                expected,
                "{tag}"
            );
        }
        assert!(newer_release(
            "1.0.9",
            Release {
                tag_name: "invalid".into(),
                draft: false,
                prerelease: false
            }
        )
        .is_err());
    }

    #[cfg(windows)]
    #[test]
    fn test_runner_has_no_package_identity() {
        assert!(!is_packaged().unwrap());
    }
}
