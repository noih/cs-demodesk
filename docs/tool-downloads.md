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
