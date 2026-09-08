# Automatic parsing

The existing periodic demo scan starts automatic parsing for entries without
analysis or a recorded error. Readiness uses Source 2 frame boundaries and a
complete DEM_Stop command, rather than elapsed time. The check reads frame
headers and skips payloads without decoding or decompressing game messages.
On Windows, a shared-read-only handle rejects active writers and remains open
through parsing. Metadata is rechecked before readiness validation. Incomplete
results remain in memory until file size or modification time changes, avoiding
repeated scans of unchanged incomplete files. No failure marker is written for
files waiting for completion.

A missing end marker can also indicate a damaged or nonstandard demo. Such files
stay unparsed automatically; users can explicitly parse them with the existing
tolerant parser. Readiness verifies framing, not the validity of game data; an
actual parse failure still requires manual retry. It does not prove that all
optional trailing metadata has arrived after the gameplay stream ends.

One automatic parse runs at a time; completion
starts the next eligible entry. Automatic results are saved to disk and their
full contents are loaded into memory only when requested.

`parsed/<demo-id>.error.json` contains one current failure, not a growing log.
A pending marker is written before parsing so an interrupted process also needs
manual retry. Success removes it. Errors survive restarts, file changes, clearing
cached analysis, and temporarily unavailable scan folders. Only an explicit Parse
request retries a failed demo. Removing a demo through the app removes its error
record. Clearing saved analysis causes demos without errors to be parsed again.

Verification: `cargo test -p demodesk-core`.
