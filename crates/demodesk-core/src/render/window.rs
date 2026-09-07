//! Hides the CS2 window while recording. Verified in practice (2026-09-08): with
//! `engine_no_focus_sleep 0` the engine keeps rendering behind a hidden window
//! (`ShowWindow(SW_HIDE)`), so HLAE still captures every frame and nothing
//! shows on screen or in the taskbar.

#[cfg(windows)]
pub fn hide_game_window(pid: u32) -> Result<bool, String> {
    use windows_sys::core::BOOL;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumWindows, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible, ShowWindow, SW_HIDE};

    struct Search {
        pid: u32,
        found: Vec<HWND>,
    }
    unsafe extern "system" fn visit(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let search = &mut *(lparam as *mut Search);
        let mut wpid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut wpid);
        if wpid == search.pid && IsWindowVisible(hwnd) != 0 {
            let mut r = RECT { left: 0, top: 0, right: 0, bottom: 0 };
            GetWindowRect(hwnd, &mut r);
            // the game window, not a tiny helper/tooltip window
            if r.right - r.left > 200 && r.bottom - r.top > 200 {
                search.found.push(hwnd);
            }
        }
        1
    }

    let mut search = Search { pid, found: vec![] };
    unsafe {
        EnumWindows(Some(visit), &mut search as *mut Search as LPARAM);
    }
    if search.found.is_empty() {
        return Ok(false);
    }
    for hwnd in search.found {
        unsafe { ShowWindow(hwnd, SW_HIDE) };
    }
    Ok(true)
}

#[cfg(not(windows))]
pub fn hide_game_window(_pid: u32) -> Result<bool, String> {
    Ok(false)
}
