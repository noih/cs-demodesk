use windows_sys::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, IDYES, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_YESNO,
};

const DOWNLOAD_URL: &str =
    "https://developer.microsoft.com/microsoft-edge/webview2/#download-section";

pub fn ready() -> bool {
    if tauri::webview_version().is_ok() {
        return true;
    }
    show_install_help();
    false
}

fn message(text: &str, flags: u32) -> i32 {
    let title: Vec<u16> = "CS DemoDesk - WebView2 Runtime"
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let text: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
    // Both strings are NUL-terminated and live until the synchronous dialog closes.
    unsafe { MessageBoxW(std::ptr::null_mut(), text.as_ptr(), title.as_ptr(), flags) }
}

fn localized_text(language: u16) -> (&'static str, &'static str) {
    // LANGID: low 10 bits identify the language; Chinese also needs its region.
    match language & 0x03ff {
        0x04 if matches!(language, 0x0404 | 0x0c04 | 0x1404 | 0x7c04) => (
            "無法找到或載入 WebView2 Runtime。\nCS DemoDesk 需要此元件才能啟動。\n\n請下載並安裝 Evergreen Bootstrapper，\n完成後重新開啟 CS DemoDesk。\n\n是否開啟 Microsoft 官方下載頁？",
            "請在瀏覽器開啟以下網址：",
        ),
        0x04 => (
            "无法找到或加载 WebView2 Runtime。\nCS DemoDesk 需要此组件才能启动。\n\n请下载并安装 Evergreen Bootstrapper，\n完成后重新打开 CS DemoDesk。\n\n是否打开 Microsoft 官方下载页？",
            "请在浏览器中打开以下网址：",
        ),
        0x11 => (
            "WebView2 Runtime が見つからないか、読み込めません。\nCS DemoDesk の起動にはこのコンポーネントが必要です。\n\nEvergreen Bootstrapper をダウンロードしてインストールし、\nCS DemoDesk を再起動してください。\n\nMicrosoft の公式ダウンロードページを開きますか？",
            "ブラウザーで次の URL を開いてください：",
        ),
        0x12 => (
            "WebView2 Runtime을 찾거나 불러올 수 없습니다.\nCS DemoDesk를 실행하려면 이 구성 요소가 필요합니다.\n\nEvergreen Bootstrapper를 다운로드하여 설치한 후\nCS DemoDesk를 다시 실행하세요.\n\nMicrosoft 공식 다운로드 페이지를 여시겠습니까?",
            "브라우저에서 다음 주소를 여세요:",
        ),
        0x19 => (
            "WebView2 Runtime не найден или не загружается.\nЭтот компонент необходим для запуска CS DemoDesk.\n\nСкачайте и установите Evergreen Bootstrapper,\nзатем перезапустите CS DemoDesk.\n\nОткрыть официальную страницу загрузки Microsoft?",
            "Откройте этот адрес в браузере:",
        ),
        _ => (
            "WebView2 Runtime is missing or could not load.\nCS DemoDesk needs this component to start.\n\nDownload and install the Evergreen Bootstrapper,\nthen restart CS DemoDesk.\n\nOpen Microsoft's official download page?",
            "Open this address in your browser:",
        ),
    }
}

fn show_install_help() {
    // This runs before the webview and app settings are available.
    let language = unsafe { windows_sys::Win32::Globalization::GetUserDefaultUILanguage() };
    let (prompt, browser_help) = localized_text(language);
    let accepted = message(prompt, MB_ICONWARNING | MB_YESNO);
    if accepted == IDYES && tauri_plugin_opener::open_url(DOWNLOAD_URL, None::<&str>).is_err() {
        message(
            &format!("{browser_help}\n{DOWNLOAD_URL}"),
            MB_ICONERROR | MB_OK,
        );
    }
}
#[cfg(test)]
mod tests {
    #[test]
    fn chooses_system_language_and_falls_back_to_english() {
        for (language, expected) in [
            (0x0404, "無法"),
            (0x0c04, "無法"),
            (0x1404, "無法"),
            (0x0804, "无法"),
            (0x1004, "无法"),
            (0x0411, "WebView2 Runtime が"),
            (0x0412, "WebView2 Runtime을"),
            (0x0419, "WebView2 Runtime не"),
            (0x0819, "WebView2 Runtime не"),
            (0x0409, "WebView2 Runtime is"),
            (0x0809, "WebView2 Runtime is"),
            (0x040c, "WebView2 Runtime is"),
            (0, "WebView2 Runtime is"),
        ] {
            let (prompt, browser_help) = super::localized_text(language);
            assert!(prompt.starts_with(expected), "LANGID {language:#06x}");
            assert!(!browser_help.is_empty());
            assert!(
                prompt.lines().all(|line| line == line.trim()),
                "unexpected indentation"
            );
        }
    }

    #[test]
    #[ignore = "Manual native dialog check; does not require uninstalling WebView2"]
    fn missing_runtime_install_guidance() {
        super::show_install_help();
    }
}
