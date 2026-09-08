#include <windows.h>
#include "detours.h"

// Window hooks target SDL; cursor hooks affect only this hidden game process.
static HANDLE logFile = INVALID_HANDLE_VALUE;
static volatile LONG recorded[10] = {};
static volatile LONG observedClasses = 0;
static decltype(&CreateWindowExW) realCreateW = CreateWindowExW;
static decltype(&CreateWindowExA) realCreateA = CreateWindowExA;
static decltype(&ShowWindow) realShow = ShowWindow;
static decltype(&ShowWindowAsync) realShowAsync = ShowWindowAsync;
static decltype(&SetWindowPos) realSetPos = SetWindowPos;
static decltype(&SetCursorPos) realSetCursor = SetCursorPos;
static decltype(&ClipCursor) realClipCursor = ClipCursor;
static decltype(&SetForegroundWindow) realForeground = SetForegroundWindow;
static decltype(&SetFocus) realFocus = SetFocus;
static decltype(&SetActiveWindow) realActive = SetActiveWindow;

static void Log(const char* text) {
    if (logFile == INVALID_HANDLE_VALUE) return;
    DWORD written;
    WriteFile(logFile, text, (DWORD)lstrlenA(text), &written, NULL);
}
static void Once(int slot, const char* text) {
    if (InterlockedCompareExchange(&recorded[slot], 1, 0) == 0) Log(text);
}
// Record a bounded class sample so an unexpected Valve class needs no extra diagnostic run.
static void ObserveClass(LPCWSTR name, DWORD style) {
    if ((style & WS_CHILD) || InterlockedIncrement(&observedClasses) > 12) return;
    char text[256];
    if (IS_INTRESOURCE(name)) { Log("create class=<atom>\n"); return; }
    if (WideCharToMultiByte(CP_UTF8, 0, name, -1, text, 256, NULL, NULL)) {
        Log("create class="); Log(text); Log("\n");
    }
}
static bool IsClassW(LPCWSTR name) {
    return !IS_INTRESOURCE(name) && lstrcmpiW(name, L"SDL_app") == 0;
}
static bool IsClassA(LPCSTR name) {
    return !IS_INTRESOURCE(name) && lstrcmpiA(name, "SDL_app") == 0;
}
static bool IsGameWindow(HWND window) {
    WCHAR name[128];
    return !(GetWindowLongPtrW(window, GWL_STYLE) & WS_CHILD)
        && GetClassNameW(window, name, 128) && IsClassW(name);
}
static HWND WINAPI HiddenCreateW(DWORD ex, LPCWSTR cls, LPCWSTR title, DWORD style,
    int x, int y, int w, int h, HWND parent, HMENU menu, HINSTANCE instance, LPVOID param) {
    ObserveClass(cls, style);
    if (!(style & WS_CHILD) && IsClassW(cls)) {
        style &= ~WS_VISIBLE;
        ex |= WS_EX_NOACTIVATE;
        Once(0, "intercepted CreateWindowExW SDL_app\n");
    }
    return realCreateW(ex, cls, title, style, x, y, w, h, parent, menu, instance, param);
}
static HWND WINAPI HiddenCreateA(DWORD ex, LPCSTR cls, LPCSTR title, DWORD style,
    int x, int y, int w, int h, HWND parent, HMENU menu, HINSTANCE instance, LPVOID param) {
    if (!(style & WS_CHILD) && IsClassA(cls)) {
        style &= ~WS_VISIBLE;
        ex |= WS_EX_NOACTIVATE;
        Once(1, "intercepted CreateWindowExA SDL_app\n");
    }
    return realCreateA(ex, cls, title, style, x, y, w, h, parent, menu, instance, param);
}
static void EnsureHidden(HWND window) {
    if (IsWindowVisible(window)) {
        realSetPos(window, NULL, 0, 0, 0, 0,
            SWP_HIDEWINDOW | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE);
    }
}
static BOOL WINAPI HiddenShow(HWND window, int command) {
    if (!IsGameWindow(window)) return realShow(window, command);
    Once(2, "intercepted ShowWindow SDL_app\n");
    const BOOL wasVisible = IsWindowVisible(window);
    // Do not call ShowWindow: STARTUPINFO can override even an initial SW_HIDE.
    EnsureHidden(window);
    return wasVisible;
}
static BOOL WINAPI HiddenShowAsync(HWND window, int command) {
    if (!IsGameWindow(window)) return realShowAsync(window, command);
    Once(3, "intercepted ShowWindowAsync SDL_app\n");
    EnsureHidden(window);
    return TRUE;
}
static BOOL WINAPI HiddenSetPos(HWND window, HWND after, int x, int y, int w, int h, UINT flags) {
    if (IsGameWindow(window)) {
        Once(4, "intercepted SetWindowPos SDL_app\n");
        flags = (flags & ~SWP_SHOWWINDOW) | SWP_HIDEWINDOW | SWP_NOACTIVATE;
    }
    return realSetPos(window, after, x, y, w, h, flags);
}

// Hidden playback must not warp the desktop pointer or change another app's clip.
static BOOL WINAPI HiddenSetCursor(int, int) {
    Once(5, "intercepted SetCursorPos\n");
    return TRUE;
}
static BOOL WINAPI HiddenClipCursor(const RECT*) {
    Once(6, "intercepted ClipCursor\n");
    return TRUE;
}

// Focus can activate even an invisible window. Keep the existing owner untouched.
static bool IsGameFocusTarget(HWND window) {
    return window && IsGameWindow(GetAncestor(window, GA_ROOT));
}
static BOOL WINAPI HiddenForeground(HWND window) {
    if (!IsGameFocusTarget(window)) return realForeground(window);
    Once(7, "intercepted SetForegroundWindow SDL_app\n");
    return FALSE;
}
static HWND WINAPI HiddenFocus(HWND window) {
    if (!IsGameFocusTarget(window)) return realFocus(window);
    Once(8, "intercepted SetFocus SDL_app\n");
    return GetFocus();
}
static HWND WINAPI HiddenActive(HWND window) {
    if (!IsGameFocusTarget(window)) return realActive(window);
    Once(9, "intercepted SetActiveWindow SDL_app\n");
    return GetActiveWindow();
}

static LONG Hooks(bool attach) {
    LONG error = DetourTransactionBegin();
    if (error != NO_ERROR) return error;
    error = DetourUpdateThread(GetCurrentThread());
#define CHANGE(real, replacement) if (error == NO_ERROR) error = attach ? \
    DetourAttach(&(PVOID&)real, replacement) : DetourDetach(&(PVOID&)real, replacement)
    CHANGE(realCreateW, HiddenCreateW);
    CHANGE(realCreateA, HiddenCreateA);
    CHANGE(realShow, HiddenShow);
    CHANGE(realShowAsync, HiddenShowAsync);
    CHANGE(realSetPos, HiddenSetPos);
    CHANGE(realSetCursor, HiddenSetCursor);
    CHANGE(realClipCursor, HiddenClipCursor);
    CHANGE(realForeground, HiddenForeground);
    CHANGE(realFocus, HiddenFocus);
    CHANGE(realActive, HiddenActive);
#undef CHANGE
    if (error != NO_ERROR) { DetourTransactionAbort(); return error; }
    return DetourTransactionCommit();
}

BOOL WINAPI DllMain(HINSTANCE, DWORD reason, LPVOID reserved) {
    if (reason == DLL_PROCESS_ATTACH) {
        // DllMain also runs on small-stack game threads; keep this buffer off their stacks.
        static WCHAR path[32768];
        DWORD length = GetEnvironmentVariableW(L"DEMODESK_WINDOW_HOOK_LOG", path, 32768);
        if (!length || length >= 32768) return FALSE;
        logFile = CreateFileW(path, FILE_APPEND_DATA, FILE_SHARE_READ | FILE_SHARE_WRITE,
            NULL, OPEN_ALWAYS, FILE_ATTRIBUTE_NORMAL, NULL);
        if (logFile == INVALID_HANDLE_VALUE) return FALSE;
        // HLAE loads this DLL while the game's main thread is suspended.
        // No worker, LoadLibrary, or waiting under the loader lock.
        if (Hooks(true) != NO_ERROR) {
            Log("installation failed\n");
            CloseHandle(logFile);
            logFile = INVALID_HANDLE_VALUE;
            return FALSE;
        }
        Log("installed\n");
    } else if (reason == DLL_PROCESS_DETACH) {
        if (!reserved) Hooks(false);
        if (logFile != INVALID_HANDLE_VALUE) CloseHandle(logFile);
    }
    return TRUE;
}
