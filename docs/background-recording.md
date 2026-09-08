# Background recording

## Process ownership

On Windows 10 or later, each recording session owns an unnamed Job Object with
`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`. HLAE is assigned to it **during creation**
using `PROC_THREAD_ATTRIBUTE_JOB_LIST`. Its descendants, including CS2 and the
recording FFmpeg processes, inherit membership. Standalone encoding and ffprobe
commands use the same launcher with a job scoped to that command.

The application retains the only job handle; it is not inherited by children.
Windows therefore terminates the associated processes when the application exits,
crashes, or is forcibly terminated, even when Rust destructors cannot execute.
The app itself and unrelated processes are not assigned to these jobs.
No breakaway permission is enabled. Failure to configure or assign a job fails
launch rather than falling back to an unmanaged process.

On ordinary recording completion, cancellation, or errors, the app terminates any
remaining members and waits up to five seconds for the tree to drain before the
next queued recording. Cleanup errors are logged. Cancellation still requests a
graceful game quit first when the console is available. Cleanup no longer uses
`taskkill /im cs2.exe`, so it does not select unrelated processes by image name.

Implementation: `crates/demodesk-core/src/render/process.rs`. This is an internal
launcher for existing output commands, not a general replacement for `Command`.
It supports their executable, CRT-quoted arguments, inherited environment with
explicit overrides/removals, and optional working directory. Stdin is NUL;
spawn-only launches use NUL output. Captured output uses anonymous temporary files
that are dropped after reading, avoiding pipe backpressure and persistent log
archives. Only the standard-I/O handles are inherited. This path requires the
Windows 10 job-list process attribute; non-Windows core tests retain ordinary
standard-library process execution.

## Hidden recording

The original HLAE injects the bundled x64 `demodesk-window-hook.dll` before
AfxHookSource2 and before resuming the game's main thread. No modified HLAE copy,
PowerShell module, user-side compiler, or extra runtime is required by the app.

The hook uses Microsoft Detours 4.0.1 to intercept Windows APIs in that recording
process. It targets the `SDL_app` top-level class for window operations, removes
visible creation/show flags, and preserves resizing. `WS_EX_NOACTIVATE` and
SetForegroundWindow/SetFocus/SetActiveWindow interception prevent hidden game
windows and their children from requesting focus. SetCursorPos and ClipCursor are
acknowledged without changing desktop cursor state. Normal shown-game mode does
not inject this DLL. Existing WinEvent hiding remains as a fallback.

Audio muting uses Windows audio sessions for the recording PID. Game-side volume
must remain available to HLAE: testing confirmed that game volume affects recorded
audio. The app isolates recording settings with `USRLOCALCSGO`, launches windowed
with `-insecure`, and preserves the user's normal game configuration.

Do not allocate large local buffers in DllMain: it also runs on thread attach.
A previous 64 KiB stack buffer caused CS2 to exit with stack overflow on small-stack
threads. The log-path buffer is static, and the native probe covers this regression.
Do not add worker startup, library loading, or waits under the loader lock.

## Queue and restart behavior

Only one render worker processes the FIFO queue. Enqueue remains available while
another job runs. Empty-queue detection and clearing the worker-running flag occur
under the same queue lock, preventing a missed wakeup at worker shutdown.
Ordinary job errors are persisted and the worker moves to the next queued job.

After an app restart, jobs that were running or queued are marked failed with a
closure error and finish time. They are not resumed automatically. Killing children
does not finalize partially recorded media; interrupted jobs still need to be
submitted again. There is no crash-recovery scan that kills an arbitrary existing
CS2 by name. Processes left by older app versions remain outside the new ownership
scheme and must be closed separately.

## Verification and maintenance

```powershell
cargo test -p demodesk-core
cargo clippy -p demodesk-core --all-targets
```

Process tests launch dedicated owner/child/grandchild fixtures. They exercise normal
job release and forcibly killing the owner, verify descendants terminate, and verify
an unrelated fixture remains running. Output tests cover quoting, Unicode, environment
values and stderr. A separate ignored test requires local game/tool paths:

```powershell
$env:DEMODESK_TEST_CS2 = 'D:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe'
$env:DEMODESK_TEST_HLAE = 'E:\path\to\HLAE.exe'
$env:DEMODESK_TEST_FFMPEG = 'E:\path\to\ffmpeg.exe'
cargo test -p demodesk-core real_recording_tools_use_managed_processes -- --ignored --nocapture
```

Close CS2 first. This checks managed HLAE injection, console echo, CS2 termination
through the job, and an FFmpeg synthetic audio-to-null conversion. It does not
replace testing a full recording in the app.

Game updates do not inherently require new offsets: the hook uses Windows APIs,
not game-internal addresses. A changed class name or API path can require a code
change. Use [compatibility diagnostics](compatibility-diagnostics.md) to collect
binary hashes, versions, class samples, event observations, and hook logs. The
scanner produces observations, never automatically patches application constants.
Keep investigation history in ignored `research/`; maintain settled guidance here
in English.

Sources:
- [Microsoft Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects)
- [Process attributes](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-updateprocthreadattribute)
- [Microsoft Detours](https://github.com/microsoft/Detours/wiki)
