# Data storage and automatic parsing

By default, the desktop app opens `demodesk-data` beside its current executable.
The Storage setting selects the data folder itself; no extra directory name is
appended. Clearing the field restores the portable default. Selection changes
apply on the next application start, so active workers keep using their original
store. Selecting a directory does not copy or move existing data.

The bootstrap preference is `data-directory.json` under Tauri's
`app.path().app_config_dir()`, outside the selected data folder. This allows the
app to discover a custom folder before opening its `settings.json`, and allows
clearing the selection while using that folder. Unwritable or relative paths are
rejected; the app does not silently switch to a different data directory. If the
selected store cannot be opened at startup, a recovery screen appears before
normal app commands or workers start. Users can choose another directory or
explicitly restore the portable default. A validated selection is saved before
restarting the app; a failed selection leaves recovery available.

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

Verification:

```powershell
cargo test -p demodesk-core
cargo test -p demodesk data_directory::tests
cargo clippy -p demodesk-core -p demodesk --all-targets -- -D warnings
npm run build
```

The parser regression uses invalid demo files to verify sequential attempts,
persisted failure state, disconnected folders, restarts, and explicit retry.
Directory tests cover selection precedence, clearing, executable relocation,
restart behavior, and rejecting relative paths or file paths.

Individually added demos stay at their original paths. The store records those
paths in `registered-demos.json`; adding a file neither copies it nor adds its
parent as a scan folder. Missing files remain registered for later availability.
Demo IDs use the existing 12-character hash of the normalized full path, so
same-named files in different directories have separate analysis records. The
list tooltip shows the source path. Hashing reads only the path, not demo content.
Moving a source file changes its ID; source relocation is not inferred.

The registration regression covers same-named files, repeated registration,
unchanged source contents, unselected siblings, and persistence across restart.

Additional regressions cover truncated frames and varints, active Windows write
handles, immediate parsing after completion, unavailable selected directories, explicit
replacement/default recovery, and preservation of the original selection/data.
