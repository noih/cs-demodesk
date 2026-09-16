use std::fs;
use std::path::{Component, Path, PathBuf};

pub fn canonical(path: &Path) -> Result<PathBuf, String> {
    let path =
        fs::canonicalize(path).map_err(|e| format!("Cannot resolve {}: {e}", path.display()))?;
    display_path(path)
}

fn display_path(path: PathBuf) -> Result<PathBuf, String> {
    // Tools such as CS2 do not all accept the Win32 verbatim prefix.
    #[cfg(windows)]
    {
        let s = path.to_str().ok_or("The data path is not valid Unicode.")?;
        if let Some(rest) = s.strip_prefix(r"\\?\UNC\") {
            return Ok(PathBuf::from(format!(r"\\{rest}")));
        }
        if let Some(rest) = s.strip_prefix(r"\\?\") {
            return Ok(PathBuf::from(rest));
        }
    }
    Ok(path)
}

#[cfg(windows)]
pub type StorageLock = std::os::windows::io::OwnedHandle;

#[cfg(windows)]
pub fn lock_root(root: &Path) -> Result<StorageLock, String> {
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_ALREADY_EXISTS},
        System::Threading::CreateMutexW,
    };
    // A lock file under the old logical root would itself be redirected into the private root.
    let name: Vec<_> = format!(
        "Global\\dev.noih.demodesk-data-{}\0",
        demodesk_core::engine::Engine::demo_id(root)
    )
    .encode_utf16()
    .collect();
    let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
    let error = unsafe { GetLastError() };
    if handle.is_null() {
        return Err(format!(
            "Cannot lock {}: {}",
            root.display(),
            std::io::Error::from_raw_os_error(error as i32)
        ));
    }
    let handle = unsafe { StorageLock::from_raw_handle(handle) };
    if error == ERROR_ALREADY_EXISTS {
        return Err(format!(
            "Another DemoDesk instance is using {}",
            root.display()
        ));
    }
    Ok(handle)
}

#[cfg(not(windows))]
pub type StorageLock = fs::File;
#[cfg(not(windows))]
pub fn lock_root(root: &Path) -> Result<StorageLock, String> {
    fs::File::open(root).map_err(|e| e.to_string())
}

#[cfg(all(windows, not(test)))]
pub fn check_other_instance() -> Result<(), String> {
    use windows_sys::{
        core::BOOL,
        Win32::{
            Foundation::{HWND, LPARAM},
            UI::WindowsAndMessaging::*,
        },
    };
    unsafe extern "system" fn visit(window: HWND, param: LPARAM) -> BOOL {
        let mut name = [0u16; 256];
        let len = unsafe { GetClassNameW(window, name.as_mut_ptr(), name.len() as i32) };
        if String::from_utf16_lossy(&name[..len.max(0) as usize]) == "dev.noih.demodesk-sic" {
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(window, &mut pid);
            }
            if pid != std::process::id() {
                unsafe {
                    *(param as *mut bool) = true;
                }
            }
        }
        1
    }
    let mut found = false;
    if unsafe { EnumWindows(Some(visit), &mut found as *mut bool as LPARAM) } == 0 {
        return Err(format!(
            "Cannot check running DemoDesk windows: {}",
            std::io::Error::last_os_error()
        ));
    }
    if found {
        return Err("Close the other Store or portable DemoDesk window, then retry.".into());
    }
    Ok(())
}

#[cfg(all(not(windows), not(test)))]
pub fn check_other_instance() -> Result<(), String> {
    Ok(())
}

pub fn same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<_> = a.as_os_str().encode_wide().collect();
        let b: Vec<_> = b.as_os_str().encode_wide().collect();
        unsafe {
            windows_sys::Win32::Globalization::CompareStringOrdinal(
                a.as_ptr(),
                a.len() as i32,
                b.as_ptr(),
                b.len() as i32,
                1,
            ) == 2
        }
    }
    #[cfg(not(windows))]
    {
        a == b
    }
}

fn relative(path: &Path, base: &Path) -> Option<PathBuf> {
    let mut parts = path.components();
    for part in base.components() {
        if !same(
            Path::new(parts.next()?.as_os_str()),
            Path::new(part.as_os_str()),
        ) {
            return None;
        }
    }
    Some(parts.collect())
}

/// Windows supplies both sides of the mapping. A package name or an I/O error is not evidence.
pub fn redirected(path: &Path) -> Result<Option<PathBuf>, String> {
    if path.components().any(|c| matches!(c, Component::ParentDir)) {
        return Err("Choose a data path without parent directory components (..).".into());
    }
    for (logical, physical) in mappings()? {
        if same(&logical, &physical) || relative(path, &physical).is_some() {
            continue;
        }
        if let Some(suffix) = relative(path, &logical) {
            if suffix.as_os_str().is_empty() {
                return Err("An AppData root cannot be used as the app data directory.".into());
            }
            return Ok(Some(physical.join(suffix)));
        }
    }
    Ok(None)
}

pub fn app_data_root(path: &Path) -> Result<bool, String> {
    for (logical, actual) in mappings()? {
        if same(path, &logical)
            || same(path, &actual)
            || logical.parent().is_some_and(|parent| {
                parent
                    .file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.eq_ignore_ascii_case("AppData"))
                    && same(path, parent)
            })
        {
            return Ok(true);
        }
    }
    Ok(false)
}

pub fn in_app_data(path: &Path) -> Result<bool, String> {
    for (logical, actual) in mappings()? {
        if relative(path, &logical).is_some() || relative(path, &actual).is_some() {
            return Ok(true);
        }
        if let Some(parent) = logical.parent() {
            if parent.file_name().is_some_and(|name| {
                name.to_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case("AppData"))
            }) && relative(path, parent).is_some()
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

#[cfg(not(windows))]
fn mappings() -> Result<Vec<(PathBuf, PathBuf)>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
fn mappings() -> Result<Vec<(PathBuf, PathBuf)>, String> {
    use windows_sys::Win32::UI::Shell::*;
    [
        FOLDERID_LocalAppData,
        FOLDERID_RoamingAppData,
        FOLDERID_LocalAppDataLow,
    ]
    .iter()
    .map(|id| {
        Ok((
            known_folder(id, KF_FLAG_NO_PACKAGE_REDIRECTION as u32)?,
            known_folder(id, KF_FLAG_RETURN_FILTER_REDIRECTION_TARGET as u32)?,
        ))
    })
    .collect()
}

#[cfg(windows)]
fn known_folder(id: &windows_sys::core::GUID, flags: u32) -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DONT_VERIFY},
    };
    let mut buffer = std::ptr::null_mut();
    let hr = unsafe {
        SHGetKnownFolderPath(
            id,
            flags | KF_FLAG_DONT_VERIFY as u32,
            std::ptr::null_mut(),
            &mut buffer,
        )
    };
    if hr < 0 {
        return Err(format!(
            "Cannot locate the Windows app data folder: 0x{:08x}",
            hr as u32
        ));
    }
    let path = unsafe {
        let mut len = 0;
        while *buffer.add(len) != 0 {
            len += 1;
        }
        let path = PathBuf::from(std::ffi::OsString::from_wide(std::slice::from_raw_parts(
            buffer, len,
        )));
        CoTaskMemFree(buffer.cast());
        path
    };
    Ok(path)
}

pub fn reparse(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

pub fn metadata(path: &Path) -> Result<Option<fs::Metadata>, String> {
    match fs::symlink_metadata(path) {
        Ok(value) => Ok(Some(value)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Cannot inspect {}: {e}", path.display())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prefix_requires_component_boundary() {
        assert_eq!(
            relative(Path::new("a/b/c"), Path::new("a/b")),
            Some(PathBuf::from("c"))
        );
        assert!(relative(Path::new("a/bb/c"), Path::new("a/b")).is_none());
    }
}
