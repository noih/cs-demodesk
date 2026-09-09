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

Individually added demos stay at their original paths. The store records those
paths in `registered-demos.json`; adding a file neither copies it nor adds its
parent as a scan folder. Missing files remain registered for later availability.
Demo IDs use the existing 12-character hash of the normalized full path, so
same-named files in different directories have separate analysis records. The
list tooltip shows the source path. Hashing reads only the path, not demo content.
Moving a source file changes its ID; source relocation is not inferred.

The official Tauri single-instance plugin is registered before other plugins
and engine setup. Reopening the app restores and focuses the existing main
window. The application identifier stays constant across locations and versions.
Recovery requests restart through the event loop so plugin exit cleanup runs.

Manual desktop checks: reopen normally and while minimized; verify the original
window returns and no second engine starts. Repeat from another portable copy,
then verify closing/reopening and data-directory recovery restart.

## Clearing and rebuilding one demo

The demo menu's Clear analysis removes the statistical result, summary and
`parsed/<demo-id>.replay.json` together. Re-parse also discards these caches and
the in-memory result before starting a fresh parse. A failed reparse leaves no
old 2D cache to reuse. Replay writes and explicit cache clearing are serialized,
and a replay request checks the current parsed state after taking that lock.
The old 2D view is unmounted while reparsing.

General statistics are rebuilt by the parse. The detailed 2D stream is generated
on demand when 2D is opened again. Shared radar assets, source demos and exported
videos are retained. Paths are relative to the selected data folder, which is
`demodesk-data` beside the executable unless changed in Settings.

## Privacy when developing and publishing

Treat demos and their derived data as private test material. Steam IDs, account
IDs, player names, profile links and personal filesystem paths can identify a
player even without credentials. Logs, screenshots, replay caches and exported
statistics can contain the same information as the source demo.

- Keep real captures and diagnostic output in ignored `out/`, `target/`,
  `research/` or `demodesk-data/` directories. Do not attach them to public issues,
  releases or CI artifacts without reviewing and sanitizing their contents.
- Use invented names and synthetic identifiers in committed tests and examples.
  Renaming a player alone does not anonymize their Steam ID or account ID.
  Do not publish a mapping back to the real identities.
- Run `npm run test:privacy` and review the staged diff before committing.
  `node scripts/check-repo-privacy.mjs --staged` checks the exact index contents.
  The scanner detects common patterns; names, images and binary files still need
  manual review. An ignored file may already be tracked in older commits.

If private data was committed, deleting the current file or adding an ignore rule
does not remove earlier copies. Sanitize every affected branch and tag, then
verify the rewritten history before replacing remote references. Keep version tag
names pointing to the corresponding sanitized commits; re-sign signed tags after
the rewrite. Changed commit hashes invalidate their original signatures.

Local cleanup is not remote cleanup. Separately review release attachments, CI
artifacts, logs and cached commit views. Existing clones can retain old objects;
avoid merging the old history back. After verification, expire local reflogs and
prune unreachable objects containing the removed data. See GitHub's
[sensitive-data removal procedure](https://docs.github.com/en/authentication/keeping-your-account-and-data-secure/removing-sensitive-data-from-a-repository)
for remote cleanup and support requests.
