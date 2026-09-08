# Hidden recording window and cursor isolation

The app embeds an x64 native DLL, built with a static C runtime and Microsoft
Detours 4.0.1. Users need no PowerShell, extra runtime installation or download.

When showGame is false (the default), the original HLAE
loader injects this DLL before AfxHookSource2 and before resuming CS2's main
thread. The existing WinEvent hider and Windows audio session mute remain.

The DLL clears WS_VISIBLE before SDL_app top-level window creation, suppresses
ShowWindow / ShowWindowAsync, and removes SWP_SHOWWINDOW while preserving
SetWindowPos position and size operations. It targets only this process's
SDL_app windows. Class atoms are never dereferenced; the upstream SDL path
uses its SDL_Appname string. Other APIs or a different Valve class can still
bypass these entry points, so CS2 must be tested.

Hidden SDL windows are created with WS_EX_NOACTIVATE. SetForegroundWindow,
SetFocus and SetActiveWindow requests targeting an SDL window (or its children)
are suppressed before they change focus. Unrelated windows keep native behavior;
there is no focus restoration loop that could override a user's later selection.

The same hidden-process DLL acknowledges SetCursorPos and ClipCursor without
changing the desktop pointer or its clipping rectangle. SDL's cursor warp uses
SetCursorPos. This does not intercept physical mouse input or other processes;
showGame bypasses this DLL entirely.

A bounded window-hook.log is written into the recording job folder: installed,
the first intercepted call of each kind, and up to twelve class samples.
The recording flow verifies installation after netcon becomes available.
The DLL installs synchronously during DLL attach while the game main thread is
suspended; it does not create a worker or wait under the loader lock.
This is a process-local hook for offline recording.

## Build and test

The normal Windows build compiles the DLL and an off-screen Win32 probe.
Only the development machine needs the MSVC toolchain.
For an offline build, set DEMODESK_DETOURS_PACKAGE to the v4.0.1 source ZIP.
Its SHA256 is enforced:
5ab84eb08fb9befeb16ffd04ca283731b1e0e1e53b1947ce7868d4b9654e43fc

cargo test -p demodesk-core --offline native_hook

The probe loads the exact bundled DLL, then checks visible creation, show and
restore calls, ShowWindowAsync, SetWindowPos plus resize, unrelated windows,
cursor warping/clipping, focus/activation (including child windows), and unloading. All windows are tool windows placed off-screen.
The user verified flash-free startup, normal recording, and cursor isolation with CS2/HLAE.

Sources:
- https://github.com/microsoft/Detours/tree/v4.0.1
- https://github.com/microsoft/Detours/wiki/Using-Detours
- https://github.com/libsdl-org/SDL/blob/main/src/video/windows/SDL_windowswindow.c

See Detours.LICENSE.md (MIT).


Cursor source: https://github.com/libsdl-org/SDL/blob/main/src/video/windows/SDL_windowsmouse.c

For update diagnostics and bounded report retention, see [scripts/README.md](../../../scripts/README.md).
