use demodesk_core::updates::Update;
use std::time::{Duration, Instant};
use windows::{
    core::Interface,
    Services::Store::StoreContext,
    Win32::{
        Foundation::HWND,
        System::Com::{CoInitializeEx, CoUninitialize, COINIT_MULTITHREADED},
        UI::Shell::IInitializeWithWindow,
    },
};
use windows_future::AsyncStatus;

pub async fn check(window: tauri::WebviewWindow) -> Result<Update, String> {
    let (send, receive) = std::sync::mpsc::channel();
    let owner = window.clone();
    // Store queries must start on the UI thread with the desktop owner window.
    window
        .run_on_main_thread(move || {
            let result = (|| {
                let hwnd = owner.hwnd().map_err(super::err)?;
                let context = StoreContext::GetDefault().map_err(super::err)?;
                let initialize: IInitializeWithWindow = context.cast().map_err(super::err)?;
                unsafe {
                    initialize.Initialize(HWND(hwnd.0)).map_err(super::err)?;
                }
                context
                    .GetAppAndOptionalStorePackageUpdatesAsync()
                    .map_err(super::err)
            })();
            if let Err(undelivered) = send.send(result) {
                if let Ok(operation) = undelivered.0 {
                    let _ = operation.Cancel();
                }
            }
        })
        .map_err(super::err)?;
    tauri::async_runtime::spawn_blocking(move || {
        unsafe {
            CoInitializeEx(None, COINIT_MULTITHREADED)
                .ok()
                .map_err(super::err)?;
        }
        struct Apartment;
        impl Drop for Apartment {
            fn drop(&mut self) {
                unsafe {
                    CoUninitialize();
                }
            }
        }
        let _apartment = Apartment;
        let timeout = Duration::from_secs(20);
        let start = Instant::now();
        let operation = receive
            .recv_timeout(timeout)
            .map_err(|_| "Microsoft Store update check timed out".to_owned())??;
        while operation.Status().map_err(super::err)? == AsyncStatus::Started {
            if start.elapsed() >= timeout {
                let _ = operation.Cancel();
                return Err("Microsoft Store update check timed out".into());
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let updates = operation.GetResults().map_err(super::err)?;
        Ok(if updates.Size().map_err(super::err)? > 0 {
            Update::StoreAvailable
        } else {
            Update::StoreCurrent
        })
    })
    .await
    .map_err(super::err)?
}
