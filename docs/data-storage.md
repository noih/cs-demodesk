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

## Match scoring

`scoring/<sha1(demoId + NUL + playerId)>/assessment-<id>.json` stores immutable
assessment snapshots. Atomic no-clobber writes preserve previous results, including
failed and unavailable assessments. Clearing parsed data, clips, or radar does not
remove this directory. Demo removal does not erase scoring history. Source changes
are detected using file metadata and a content SHA-1; history remains readable through
`scoring_history` without requiring the source file. Source validation errors remain
visible when the UI falls back to history. No replay jump is offered for these records.

Opening a demo, the scoring tab, or another player only reads saved history.
The detail header's Analyze scores button explicitly runs `score_match` for every
player together. Pressing it again replaces the complete match result; existing demo
basic auto-analysis remains unchanged. No evaluated rules means no score.

New runs publish all players atomically in
`behavior-analysis/matches/<sha1(demoId)>/latest.json`; older run copies are removed after successful replacement. Generic source data is cached independently in
`analysis/match-state/<sha1(sourceFingerprint)>-v6-0.10.0.gz`.
See [format, measured budgets and remaining limitations](analysis-performance.md).
This does not yet replace the body measurement journal; both input sizes are
reported explicitly when diagnostic measurements are used.

Diagnostic producer inputs live in `analysis/scoring-inputs/<sha1(sourceFingerprint +
NUL + playerId)>.json`. The `scoring-measurements` artifact contains per-rule inputs
and producer/source versions; the assessment preserves their metadata and input-file
hash. These inputs are currently produced by an explicit offline capture workflow,
not automatically generated by opening a demo. The crosshair rule remains uncalibrated
and does not issue credit deductions, even when diagnostic observations exist.

The preferred input is now the shared delta journal
`analysis/body-measurements/<sha1(sourceFingerprint)>.ndjson`. The native Rust
reader verifies its footer and streams current measurements into the rule; it does
not materialize every observer/target pair for the match. The journal takes precedence
when present; legacy per-player JSON remains a compatibility fallback. Input metadata,
dependencies and the complete journal content hash accompany every assessment. On
Windows the journal stays open without write/delete sharing throughout evaluation.
Evidence in `experimental-2` contains a bounded excerpt plus `sampleCount` and complete
interval metrics; the original values remain in the referenced source artifacts.

New exports use `.ndjson.gz` with lossless gzip; the loader prefers that file when
present and keeps uncompressed journals as a compatibility fallback. Body-journal
v2 retains per-window alignment diagnostics in its footer. Precision experiments do
not change stored production measurements: rounding can change threshold-boundary
candidates even at four decimal places. See `analysis-data.md` for measured results.

## Anomaly analysis queue

Analysis jobs stay in memory for the current app session; they are never saved or restored. Startup removes the former `analysis-jobs.json` file. The manual `score_match` command returns immediately; `analysis_jobs` supplies the current snapshot and `analysis-job-changed` supplies revisions and live steps. One worker analyzes a whole match at a time. Active requests for the same demo are deduplicated; failures retain their cause for this session and do not block the next match. Saved analysis results and the video render queue remain separate. Basic demo auto-analysis is independent.

## Anomaly video exports

Anomaly videos use the existing highlight render queue and renderer. A manual export selects one player and saved assessment plus observed rule IDs. By default each selected rule produces a separate queued job with merging enabled. The optional cross-rule merge creates one job, retaining rule groups and their padded boundaries; the job stores its clip snapshot in `analysisClips`, so later analysis does not change a queued export. Overlapping windows in the same round are joined. Each rule video retains 3 seconds before the first event and after the last event, and 1.5 seconds on the internal sides of intervening clips, bounded by the available timeline. The source content fingerprint is checked before recording. Normal highlight selection and automatic basic analysis remain independent.

設定的「異常數據」容量合計 `analysis/` 與 `behavior-analysis/`；清空會刪除共用分析資料和分析結果，保留基本解析與影片。分析排隊或執行中不允許清空。資料夾按鈕開啟 `analysis/`。

每場異常分析只保存 `behavior-analysis/matches/<sha1(demoId)>/latest.json`。完整新結果寫入並同步後才原子取代舊結果，再清除該場舊 `match-*.json` 副本。分析失敗不移除既有結果；排隊影片使用獨立片段快照。

工作錯誤代碼統一由 `demodesk_core::ErrorCode` 定義，序列化為 `errorCode`，前端共用 `errors.<code>` 翻譯；`error` 保留診斷內容，未知代碼不比對英文句子。
