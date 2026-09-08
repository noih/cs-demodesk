#include <windows.h>
#include <stdio.h>
#include <initializer_list>

static void resultLog(const char* message) {
    WCHAR path[32768];
    if (!GetEnvironmentVariableW(L"DEMODESK_WINDOW_HOOK_LOG", path, 32768)) return;
    HANDLE file = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE, NULL, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
    if (file == INVALID_HANDLE_VALUE) return;
    DWORD written;
    WriteFile(file, message, (DWORD)lstrlenA(message), &written, NULL);
    WriteFile(file, "\n", 1, &written, NULL);
    CloseHandle(file);
}
static int failure(const char* message) { resultLog(message); fprintf(stderr, "%s\n", message); return 1; }
int wmain(int argc, wchar_t** argv) {
    if (argc != 2) return failure("expected hook DLL");
    HMODULE hook = lstrcmpW(argv[1], L"--injected") == 0 ? GetModuleHandleW(L"demodesk-window-hook.dll") : LoadLibraryW(argv[1]);
    if (!hook) return failure("LoadLibrary hook failed");
    POINT cursorBefore, cursorAfter;
    if (!GetCursorPos(&cursorBefore)) return failure("GetCursorPos failed");
    if (!SetCursorPos(cursorBefore.x + 2, cursorBefore.y + 2) || !GetCursorPos(&cursorAfter)) return failure("cursor request failed");
    if (cursorBefore.x != cursorAfter.x || cursorBefore.y != cursorAfter.y) {
        SetCursorPos(cursorBefore.x, cursorBefore.y);
        return failure("hidden process moved the cursor");
    }
    RECT clipBefore, clipAfter;
    if (!GetClipCursor(&clipBefore)) return failure("GetClipCursor failed");
    RECT requestedClip = { cursorBefore.x, cursorBefore.y, cursorBefore.x + 10, cursorBefore.y + 10 };
    if (!ClipCursor(&requestedClip) || !GetClipCursor(&clipAfter)) return failure("clip request failed");
    if (!EqualRect(&clipBefore, &clipAfter)) {
        ClipCursor(&clipBefore);
        return failure("hidden process changed cursor clipping");
    }
    if (!ClipCursor(NULL) || !GetClipCursor(&clipAfter) || !EqualRect(&clipBefore, &clipAfter))
        return failure("hidden process released cursor clipping");
    // CS2 starts threads with small stacks; DLL_THREAD_ATTACH must fit them too.
    HANDLE smallThread = CreateThread(NULL, 65536, [](LPVOID) -> DWORD { return 0; }, NULL, STACK_SIZE_PARAM_IS_A_RESERVATION, NULL);
    if (!smallThread || WaitForSingleObject(smallThread, 5000) != WAIT_OBJECT_0) return failure("small-stack thread initialization failed");
    CloseHandle(smallThread);
    WNDCLASSW cls = {};
    cls.hInstance = GetModuleHandleW(NULL);
    cls.lpfnWndProc = DefWindowProcW;
    cls.lpszClassName = L"SDL_app";
    if (!RegisterClassW(&cls)) return failure("RegisterClass failed");
    HWND window = CreateWindowExW(WS_EX_TOOLWINDOW, cls.lpszClassName, L"DemoDesk hook test",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE, -32000, -32000, 640, 480,
        NULL, NULL, cls.hInstance, NULL);
    if (!window || IsWindowVisible(window)) return failure("visible creation was not blocked");
    if (!(GetWindowLongPtrW(window, GWL_EXSTYLE) & WS_EX_NOACTIVATE)) return failure("missing no-activate style");
    const HWND foregroundBefore = GetForegroundWindow();
    const HWND focusBefore = GetFocus();
    const HWND activeBefore = GetActiveWindow();
    SetForegroundWindow(window);
    SetFocus(window);
    SetActiveWindow(window);
    HWND child = CreateWindowExW(0, L"STATIC", L"Child", WS_CHILD, 0, 0, 10, 10,
        window, NULL, cls.hInstance, NULL);
    if (!child) return failure("child creation failed");
    SetFocus(child);
    if (GetForegroundWindow() != foregroundBefore || GetFocus() != focusBefore || GetActiveWindow() != activeBefore)
        return failure("hidden window changed focus or activation");
    DestroyWindow(child);
    for (int mode : { SW_SHOW, SW_RESTORE, SW_SHOWDEFAULT, SW_SHOWMAXIMIZED }) {
        if (ShowWindow(window, mode) || IsWindowVisible(window)) return failure("ShowWindow failed");
    }
    if (!ShowWindowAsync(window, SW_SHOW) || IsWindowVisible(window)) return failure("ShowWindowAsync failed");
    if (!SetWindowPos(window, NULL, -32000, -32000, 800, 600, SWP_SHOWWINDOW | SWP_NOZORDER))
        return failure("SetWindowPos failed");
    RECT rect;
    if (IsWindowVisible(window) || !GetWindowRect(window, &rect) ||
        rect.right - rect.left != 800 || rect.bottom - rect.top != 600)
        return failure("hidden resize was not preserved");
    HWND ansi = CreateWindowExA(WS_EX_TOOLWINDOW, "SDL_app", "ANSI test", WS_OVERLAPPEDWINDOW | WS_VISIBLE,
        -32000, -32000, 320, 240, NULL, NULL, cls.hInstance, NULL);
    if (!ansi || IsWindowVisible(ansi)) return failure("ANSI create failed");
    HWND other = CreateWindowExW(WS_EX_TOOLWINDOW, L"STATIC", L"Other",
        WS_OVERLAPPEDWINDOW | WS_VISIBLE, -32000, -32000, 320, 240,
        NULL, NULL, cls.hInstance, NULL);
    if (!other || !IsWindowVisible(other)) return failure("unrelated class changed");
    DestroyWindow(other);
    DestroyWindow(ansi);
    if (!FreeLibrary(hook)) return failure("unload failed");
    ShowWindow(window, SW_SHOW);
    if (!IsWindowVisible(window)) return failure("hooks were not detached");
    DestroyWindow(window);
    resultLog("probe passed");
    puts("Win32 create/show/async/position/resize, non-target and unload checks passed");
    return 0;
}
