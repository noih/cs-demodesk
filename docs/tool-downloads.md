# Tool downloads

Resolve download URLs from GitHub release asset metadata rather than constructing
URLs from version numbers. BtbN's rolling `latest` tag is preferred; GitHub's
latest published release is the fallback. These are distinct endpoints:
`releases/download/latest/<asset>` targets the rolling tag, whereas
`releases/latest/download/<asset>` follows GitHub's latest release selection.

Choose exactly one compatible asset: FFmpeg win64 GPL static, HLAE portable ZIP,
and Source 2 Viewer CLI for the target platform. Reject missing or ambiguous
matches and report available asset names instead of guessing an architecture or
variant. A download returning 404 or 410 refreshes metadata once. Other failures
are reported and can be retried with Download tools.

Downloads report received MB separately from the log on the first chunk and about
once per second while data arrives, including when Content-Length is unavailable.
Each tool can download independently; only duplicate downloads of the same tool
are blocked. Each description row shows its own progress beside download, and
the download button is replaced by cancel while running. Cancellation is checked
between network reads and before installation replaces the old tool. Network
input waits poll cancellation every 250 ms and stop after 60 seconds without
input; connection establishment still uses its own timeout. Cancelled downloads
remove their staging directory and leave existing tools intact.
Readiness stays on the title row. Each tool retains its own log, containing only
start and completion, cancellation, or error. Opening the log is optional;
starting a download does not open it.
Connections have a
30-second timeout and response headers a 60-second timeout. Archive downloads
have a two-hour total limit (not an idle timeout); metadata requests retain the
10-minute total limit. Incomplete bodies are rejected before extraction.
The settings guide follows download phase changes (log icon, progress, finish),
without rebuilding for each progress update, and closes once the required tools
are ready.

At startup and when selecting a data directory, the app checks directory listing
and file creation, writing, reading, and removal in the data root, tools, parsed,
clips, and existing tool directories. Failures identify the path through the
existing startup recovery screen or settings error. This checks app access, not
whether Windows allows a downloaded executable to initialize.

Settings separately verifies executable startup on entry, after saving paths,
and after each tool installation. HLAE uses `-customLoader -noGui -noConfig`,
Source 2 Viewer uses `--version`, and FFmpeg uses `-version`. Checks have a
10-second deadline and capture up to 16 KiB per output stream. Windows uses the
same owned process tree as rendering, so timed-out probes and their children are
terminated. A successful launch is not a decoding or game-injection guarantee.
Unverified and failed tools do not show the ready status in Settings. Cached
results are invalidated when the executable or required companion file changes
or disappears; Check again reruns the probes without downloading anything.

Diagnostics shows a reviewable report with app/package identity, Windows build,
.NET Framework release, configured/resolved paths, check times, exit codes,
captured output and download errors. Copying is explicit; reports are neither
uploaded nor persisted. Paths can contain usernames. Re-download uses the existing
tool download button; alternate tool locations use Browse and Save.

`cargo test -p demodesk-core --lib diagnostics` exercises failed executables,
missing companion files, deleted files, output limits and hung processes.
For actual installed tools, set `DEMODESK_TEST_TOOLS_DIR` and run
`cargo test -p demodesk-core installed_tools_startup -- --ignored --nocapture`.
The test host is not MSIX-packaged: compare reports from installed and portable
apps on the affected machine to determine whether package context matters.

Each tool downloads into one fixed `.installing` sibling directory. The ZIP must
extract successfully and contain the required binaries before installation is
replaced. HLAE needs HLAE.exe and x64/AfxHookSource2.dll; FFmpeg needs ffmpeg.exe
and ffprobe.exe in the same directory; Source 2 Viewer needs its CLI executable.
An incomplete directory does not count as an installed tool.

Replacement moves the old directory to one `.previous` sibling. A failed move
restores it; the next setup recovers an interrupted swap. Successful installs
remove the staging directory and previous version when possible. Failed staging
is replaced on retry, keeping storage bounded. install-info.json records the tag,
resolved asset URL and installation time. Existing older files under downloads/
are not automatically deleted.

Verification: `cargo test -p demodesk-core render::setup::tests` covers asset
selection, incomplete installs, corrupt ZIPs, rollback and interrupted swaps.
