//! Hide CS2 in response to WinEvents, with one initial scan after registration.
use std::sync::{atomic::AtomicU32, mpsc, Arc};

#[cfg(windows)]
mod platform {
    use super::*;
    use std::cell::RefCell;
    use std::sync::atomic::Ordering;
    use std::thread::JoinHandle;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::Accessibility::{SetWinEventHook, UnhookWinEvent, HWINEVENTHOOK};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    thread_local! {
        static TARGET: RefCell<Option<(u32, Arc<AtomicU32>)>> = const { RefCell::new(None) };
    }

    unsafe fn hide_if_game(hwnd: HWND, pid: u32, hidden: &AtomicU32) {
        if hwnd.is_null() || GetAncestor(hwnd, GA_ROOT) != hwnd || IsWindowVisible(hwnd) == 0 {
            return;
        }
        let mut owner = 0;
        GetWindowThreadProcessId(hwnd, &mut owner);
        if owner != pid {
            return;
        }
        let mut rect: RECT = std::mem::zeroed();
        if GetWindowRect(hwnd, &mut rect) == 0
            || rect.right - rect.left <= 200
            || rect.bottom - rect.top <= 200
        {
            return;
        }
        ShowWindow(hwnd, SW_HIDE);
        if IsWindowVisible(hwnd) == 0 {
            hidden.fetch_add(1, Ordering::Relaxed);
        }
    }

    unsafe extern "system" fn on_event(
        _hook: HWINEVENTHOOK,
        event: u32,
        hwnd: HWND,
        object: i32,
        child: i32,
        _thread: u32,
        _time: u32,
    ) {
        if object != OBJID_WINDOW
            || child != 0
            || !matches!(
                event,
                EVENT_OBJECT_CREATE | EVENT_OBJECT_SHOW | EVENT_OBJECT_LOCATIONCHANGE
            )
        {
            return;
        }
        // Release the borrow before ShowWindow, which can cause reentrant callbacks.
        let target = TARGET.with(|target| target.borrow().clone());
        if let Some((pid, hidden)) = target {
            hide_if_game(hwnd, pid, &hidden);
        }
    }

    unsafe extern "system" fn initial_window(
        hwnd: HWND,
        _param: LPARAM,
    ) -> windows_sys::core::BOOL {
        let target = TARGET.with(|target| target.borrow().clone());
        if let Some((pid, hidden)) = target {
            hide_if_game(hwnd, pid, &hidden);
        }
        1
    }

    pub struct EventHider {
        thread_id: u32,
        worker: Option<JoinHandle<()>>,
        messages: mpsc::Sender<String>,
    }

    impl EventHider {
        pub fn start(
            pid: u32,
            hidden: Arc<AtomicU32>,
            messages: mpsc::Sender<String>,
        ) -> Result<Self, String> {
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            let worker_messages = messages.clone();
            let worker = std::thread::Builder::new()
                .name("game-window-events".into())
                .spawn(move || unsafe {
                    let mut msg: MSG = std::mem::zeroed();
                    // Create the queue before publishing its thread ID for shutdown.
                    PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                    TARGET.with(|target| *target.borrow_mut() = Some((pid, hidden)));
                    let hook = SetWinEventHook(
                        EVENT_OBJECT_CREATE,
                        EVENT_OBJECT_LOCATIONCHANGE,
                        std::ptr::null_mut(),
                        Some(on_event),
                        pid,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    );
                    if hook.is_null() {
                        let _ = ready_tx.send(Err(std::io::Error::last_os_error().to_string()));
                        TARGET.with(|target| *target.borrow_mut() = None);
                        return;
                    }
                    // Catch windows created before the hook was installed, without a polling loop.
                    EnumWindows(Some(initial_window), 0);
                    if ready_tx.send(Ok(GetCurrentThreadId())).is_ok() {
                        let _ = worker_messages.send("CS2 window event listener ready".into());
                        loop {
                            let result = GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0);
                            if result <= 0 {
                                if result == -1 {
                                    let _ = worker_messages.send(format!(
                                        "warning: window event loop failed: {}",
                                        std::io::Error::last_os_error()
                                    ));
                                }
                                break;
                            }
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                    }
                    UnhookWinEvent(hook);
                    TARGET.with(|target| *target.borrow_mut() = None);
                })
                .map_err(|e| e.to_string())?;
            match ready_rx.recv() {
                Ok(Ok(thread_id)) => Ok(Self {
                    thread_id,
                    worker: Some(worker),
                    messages,
                }),
                result => {
                    let _ = worker.join();
                    Err(match result {
                        Ok(Err(error)) => error,
                        Err(error) => error.to_string(),
                        _ => unreachable!(),
                    })
                }
            }
        }

        pub fn stop(&mut self) {
            if let Some(worker) = self.worker.take() {
                if !worker.is_finished()
                    && unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0) } == 0
                {
                    let _ = self.messages.send(format!(
                        "warning: could not stop window event listener: {}",
                        std::io::Error::last_os_error()
                    ));
                    self.worker = Some(worker);
                    return;
                }
                let _ = worker.join();
            }
        }
    }

    impl Drop for EventHider {
        fn drop(&mut self) {
            self.stop();
        }
    }
}

#[cfg(windows)]
pub use platform::EventHider;

#[cfg(not(windows))]
pub struct EventHider;
#[cfg(not(windows))]
impl EventHider {
    pub fn start(
        _pid: u32,
        _hidden: Arc<AtomicU32>,
        _messages: mpsc::Sender<String>,
    ) -> Result<Self, String> {
        Err("window events require Windows".into())
    }
    pub fn stop(&mut self) {}
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::time::{Duration, Instant};
    use windows_sys::Win32::System::Threading::{GetCurrentProcessId, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::*;

    struct TestWindow {
        thread_id: u32,
        hwnd: isize,
        worker: Option<std::thread::JoinHandle<()>>,
    }
    impl TestWindow {
        fn start() -> Self {
            let (tx, rx) = mpsc::channel();
            let worker = std::thread::spawn(move || unsafe {
                let class: Vec<u16> = "STATIC\0".encode_utf16().collect();
                let mut msg: MSG = std::mem::zeroed();
                PeekMessageW(&mut msg, std::ptr::null_mut(), 0, 0, PM_NOREMOVE);
                // Off-screen window exercises real WinEvents without flashing on the desktop.
                let hwnd = CreateWindowExW(
                    0,
                    class.as_ptr(),
                    class.as_ptr(),
                    WS_OVERLAPPEDWINDOW | WS_VISIBLE,
                    -32000,
                    -32000,
                    640,
                    480,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null(),
                );
                tx.send((GetCurrentThreadId(), hwnd as isize)).unwrap();
                if hwnd.is_null() {
                    return;
                }
                while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                    if msg.message == WM_APP {
                        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    } else {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                DestroyWindow(hwnd);
            });
            let (thread_id, hwnd) = rx.recv_timeout(Duration::from_secs(5)).unwrap();
            assert_ne!(hwnd, 0, "test window creation failed");
            Self {
                thread_id,
                hwnd,
                worker: Some(worker),
            }
        }
        fn show(&self) {
            assert_ne!(
                unsafe { PostThreadMessageW(self.thread_id, WM_APP, 0, 0) },
                0
            );
        }
    }
    impl Drop for TestWindow {
        fn drop(&mut self) {
            unsafe {
                PostThreadMessageW(self.thread_id, WM_QUIT, 0, 0);
            }
            if let Some(worker) = self.worker.take() {
                worker.join().unwrap();
            }
        }
    }
    fn wait_until(mut condition: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(5);
        while !condition() {
            assert!(Instant::now() < deadline, "window event timed out");
            std::thread::sleep(Duration::from_millis(10));
        }
    }
    #[test]
    fn events_hide_existing_and_reshown_window_and_stop_cleanly() {
        let window = TestWindow::start();
        let hidden = Arc::new(AtomicU32::new(0));
        let (tx, _rx) = mpsc::channel();
        let mut hider =
            EventHider::start(unsafe { GetCurrentProcessId() }, hidden.clone(), tx).unwrap();
        wait_until(|| hidden.load(Ordering::Relaxed) >= 1);
        let before = hidden.load(Ordering::Relaxed);
        window.show();
        wait_until(|| hidden.load(Ordering::Relaxed) > before);
        assert_eq!(unsafe { IsWindowVisible(window.hwnd as _) }, 0);
        hider.stop();
        window.show();
        wait_until(|| unsafe { IsWindowVisible(window.hwnd as _) } != 0);
        std::thread::sleep(Duration::from_millis(250));
        assert_ne!(unsafe { IsWindowVisible(window.hwnd as _) }, 0);
    }
}
