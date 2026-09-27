# Development diagnostics

Analysis design and measured limits: [Analysis compatibility plan](analysis-compatibility-plan.md).

## Offline animation failure diagnostics

The `analysis_native_check` example can replay an existing compact match-state file
without starting CS2. Add `--diagnose REPORT.json` to record up to three failures
per reason with packet ordinal, demo/network/animation ticks, entity index and
serial, recipe version, graph handle, cache read/write IDs, and compact field IDs.
The report also records the compact contract, recorded source build/patch and
fingerprint, current client hash, game content fingerprint, asset byte count,
aggregate coverage, and preceding cache writes for sampled cache reads.
It records no player name or Steam ID. The report is capped at 2 MiB; an oversized
report fails instead of silently truncating. Keep it with the private compact file
in an ignored `out/` directory.

```powershell
cargo run -p demodesk-core --example analysis_native_check -- STATE.gz GAME_ROOT VRF_EXE CACHE_DIR --diagnose out/diagnostic-latest.json
```

Use the same command without `--diagnose` for ordinary aggregate coverage. The
reason codes are `cached_pose_missing`, `pose_tick_mismatch`,
`unsupported_animation`, `resource_or_dictionary_missing`,
`pose_input_missing_or_invalid`, and `other_reconstruction_error`. The original
error text remains in `unavailable`; a reason code identifies a class of failure,
not proof of its cause. The packet ordinal and field IDs refer to the supplied
compact stream (zero-based field IDs, one-based packet ordinal).

Run the analysis compatibility gate with the same recorded state and an optional
full-match assessment export. It reports `unverified` until a reviewed baseline
is explicitly created; missing input reports `not-run`.

```powershell
powershell.exe -NoProfile -File scripts/check-analysis-compatibility.ps1 -State STATE.gz -Game GAME_ROOT -Vrf VRF_EXE -Cache CACHE_DIR -Report out/analysis-compatibility/latest.json
node scripts/check-analysis-compatibility.mjs out/analysis-compatibility/latest.json --assessments ASSESSMENTS.json --accept out/analysis-compatibility/baseline.json
powershell.exe -NoProfile -File scripts/check-analysis-compatibility.ps1 -State STATE.gz -Game GAME_ROOT -Vrf VRF_EXE -Cache CACHE_DIR -Assessments ASSESSMENTS.json -Baseline out/analysis-compatibility/baseline.json
```

For an independently captured attachment log, compare against a native range
export and the exact model's attachment definitions. The clock shift is measured
from eye/view alignment before this command; it is not chosen to improve bone
error. Other player models are reported separately and are not compared with
definitions for this model.

```powershell
node scripts/compare-native-attachments.mjs CONSOLE.log NATIVE.json MODEL.json CLOCK_SHIFT MAX_ERROR out/analysis-compatibility/attachment-result.json
```

## CS2 update compatibility

`check-cs2-compatibility.ps1` runs on built-in Windows PowerShell 5.1 with .NET.
It needs no Python, PowerShell 7, downloaded modules, or administrator rights.
The C# companion observes WinEvents and verifies the launched process; it is
compiled by `Add-Type` on the development machine and is not shipped in the app.

Close CS2 first. Build the current core, then pass the exact binaries to test:

```powershell
cargo build -p demodesk-core
$hook = Get-ChildItem target/debug/build -Recurse -Filter demodesk-window-hook.dll |
  Sort-Object LastWriteTime -Descending | Select-Object -First 1 -ExpandProperty FullName
powershell.exe -NoProfile -File scripts/check-cs2-compatibility.ps1 -Cs2 'D:\SteamLibrary\steamapps\common\Counter-Strike Global Offensive\game\bin\win64\cs2.exe' -Hlae 'target/debug/demodesk-data/tools/hlae/HLAE.exe' -Hook $hook
```

Adjust the CS2 and HLAE paths for the machine. `-TimeoutSeconds` defaults to 60;
`-ObserveSeconds` defaults to 8 seconds after the console responds to a unique
`echo` challenge. The report records the actual command, environment overrides,
Windows/PowerShell versions, Steam build ID where available, binary versions,
SHA256, verified PID, timing, exit codes, and window events (class, rectangle,
visibility; no window titles or other apps' keystrokes).

The tool starts the original HLAE with the current native hook before AfxHookSource2,
`-insecure`, windowed mode, a temporary netcon port, and isolated `USRLOCALCSGO`.
It only controls a verified direct CS2 child of its own HLAE process. If ownership
cannot be verified it reports failure rather than killing a process by name.
The observer is event-driven. PID discovery and console readiness use bounded waits.

### Retention and baseline

Output is fixed at `target/compatibility/` (or `-OutputDir`):

- `latest/report.md`: readable verdict, differences, errors and scope.
- `latest/report.json`: detailed machine-readable evidence.
- `latest/window-hook.log`: bounded first-call hook evidence.
- `latest/netcon.log`: maximum 2 MiB; exceeding this fails the scan.
- `latest/hlae.log`: at most 1,048,576 characters, with a truncation flag.
- `latest/cfg/`: isolated settings from this one test run.
- `baseline.json`: one accepted comparison summary, with no event/log history.

Each scan replaces **only** the marked `latest` directory. There are no timestamped
history directories or rolling baseline backups. Copy a report elsewhere yourself
if you need to preserve it. A directory lock prevents concurrent scan/accept runs;
unmarked directories and reparse points inside `latest` are refused.
Window events are limited to 5,000; overflow fails the scan. No `-condebug` is used,
so this diagnostic does not produce a growing game-directory `console.log`.

After manually checking recording, audio, cursor and focus behavior in the app,
accept the latest successful run as the comparison baseline:

```powershell
powershell.exe -NoProfile -File scripts/check-cs2-compatibility.ps1 -Mode Accept
```

Acceptance is explicit, rejects failed reports, and atomically replaces the single
summary. The next scan compares binary hashes, Windows/Steam versions, window
classes and intercepted calls against it. Changes are observations, not automatic
reasons to patch constants. The tool never modifies application settings or hooks.

### Debugging from a report

Start with `report.md`, then inspect `report.json` and the three raw logs:

1. Compare hook/CS2/HLAE/Afx hashes to distinguish a stale build from a game update.
2. Check hook installation and window creation interception. A new class is a
   candidate for investigation, not automatically the game window.
3. Check visible-window/foreground events and the ordering of the hook log.
4. Check console echo timing, early exit codes and loader output for startup failures.
5. Reproduce in the app when only recording/audio/input fails.

This is a startup test, not proof that every frame remains invisible or that
recording works. WinEvents are observed after PID discovery, with one initial
window enumeration; the injected DLL supplies earlier bounded class samples.
Only existing hook points are logged, so entirely new API paths are not discovered
automatically. Missing calls in one run do not prove an API was removed.
The tool does not run the app's Windows audio mute or fallback window hider.
Report paths and game/loader output may contain local information; inspect before
sharing. Windows crash dumps generated by CS2 are outside this tool's retention.

### Regression checks

```powershell
powershell.exe -NoProfile -File scripts/test-compatibility.ps1
```

Covers hash/class/API differences, unchanged input, failed-baseline rejection,
baseline replacement without log history, active-file sharing, failed scan reports,
and replacing stale scan artifacts. The scan itself compiles the native observer.

## Statistics and recoil data after a CS2 update

Use the [Recoil calibration SOP](recoil-calibration.md) for fixed-reference
capture and updates. The chart reads the bundled calibration, not the legacy
parsed `recoilReference` field. Calibration-only changes do not invalidate match
statistics caches; restore the accepted JSON from Git to roll back.

Parser changes that alter cached statistics or player trajectories require the
appropriate `PARSED_SCHEMA_VERSION` bump in `store.rs`. Validate old/new demo
fixtures rather than assuming successful parsing proves field semantics are
unchanged. Do not change expected statistics to hide regressions. Reparse derived
analysis after a parser rollback; retain original recordings, settings and videos.

Keep private recordings and investigation output outside version control. See
[scripts README](../scripts/README.md#private-test-data) for handling rules.

## Existing application logs

Each render job retains at most 400 UI log lines in its job record. Those small
logs remain with the job until it is deleted. Native hook logs record each API
once plus at most 12 class samples. CS2 `console.log` is reset before recording;
it is a single-run log but has no byte cap while the game is running. Setup logs
are cleared at the next setup operation. The diagnostic retention above applies
to this developer tool, not to deleting existing render jobs or their videos.

## Native animation compatibility (2026-09-27)

Recipe versions 2 and 3 share the supported network-tick/task prefix. The decoder
still checks dictionaries, indices, dependencies and payload boundaries, preserves
opaque trailing bytes, and rejects other versions. Accepting a recipe version does
not authorize an unverified client implementation.

The current client SHA-256 is
`9b4f46dbd6a433163b39d7ea0123c321b1ad6d95ceedd40ae121312464833549`.
AimCS, SnapWeapon and FootIK were compared with the previously traced client:
FootIK deserialize/execute are at RVAs `0x13abec0`/`0x13ac140`, AimCS at
`0x70be70`/`0x70c800`, and SnapWeapon execute at `0x70cbe0`. The FootIK and
SnapWeapon instruction sequences match after address relocation; AimCS helper
changes also required an independent game-output comparison.

A fresh offline replay using HLAE 2.192.6 supplied 128 frames for two SAS models,
with ten attachments per player. The model bytes match the existing attachment
definitions. Eye/view alignment uniquely selected packet tick = render tick - 3;
this measured offset is diagnostic only and is not hardcoded into analysis.
All 2,560 attachment positions met the existing tolerances: maximum head/torso/leg
error 0.000606 game units (limit 0.001), maximum hand error 0.002210 (limit 0.01).
That earlier recipe-scoped implementation reconstructed 1,054,290 of 1,057,475
pawn frames. After tracing preceding cache writes and preserving pose state within
one continuous pawn lifetime, 1,057,247 frames reconstruct; 40 still lack a
verified cache source and 188 have mismatched timestamps. A separate full-range
attachment comparison for the same model measured 2,560 pairs with maximum error
0.002210 game units. Other models were excluded from this model-specific comparison.

Recheck with `npm run test:core -- --lib` and the existing `analysis_native_check`
example. Its optional `FIRST_TICK LAST_TICK OUTPUT.json` arguments export bounded
world-space hitbox bone transforms for comparison with
`scripts/capture-analysis-attachments.mjs`; model IDs are exact decimal strings.
Keep real captures and exports in ignored directories. This run's local evidence
is in `out/animation-path-fix/` (`binary-comparison.json`, `native-v3.log`,
`oracle/comparison.json`). The scoring version `18-animation-recipe-v3` invalidates
previous partial assessments for reuse while preserving their historical records.

The desktop scoring path completed for all ten players, with all three aim rules
receiving samples and 216 TTD samples. Cache reuse and forced reassessment returned
consistent results. The debug-profile acceptance run did fail its existing timing
gate: preparation took 35.59 seconds and analysis 43.73 seconds (30 seconds each
allowed); this is functional verification, not a performance acceptance pass.

### Smoke sample follow-up

Two independent omissions prevented directional smoke samples: native preparation
did not request `scripts/weapons.vdata`, and the replicated spread-policy reader
accepted patch 14181 only. Native preparation now includes the weapon dependency.
Patch 14185 uses the same three spread defaults, checked in an isolated game
process (build 10924, revision 11039926): shotgun patterns enabled, only-up disabled,
maximum inaccuracy disabled. The console capture is kept locally at
`out/animation-path-fix/smoke-policy/console.log`. Recorded overrides and signon
ordering still apply; unknown patches remain rejected. This spread verification
does not authorize the separate rewind defaults for patch 14185.

Compact producer version 0.25.0 and scoring version `19-smoke-weapon-policy`
invalidate reusable results produced without those inputs. The regression test
`current_patch_preserves_recorded_spread_policy` failed before the patch support
was added; `analysis_native_check` also requires the loaded weapon dependency.
On the reported match, rebuilding the compact source restored 182 directional
candidates and 171 eligible smoke shots after the normal obstruction checks.
Eight players have an estimated hit rate; two have no eligible denominator, so
their confirmed smoke-hit events remain evidence without a fabricated percentage.
TTD still has 216 samples. Core validation: 295 passed, 6 ignored.
The desktop-path rerun also passed cache reuse, forced-result equality and replay
preservation checks. Its unchanged timing gate still fails: first preparation
39.87 seconds, analysis 43.17 seconds; warm preparation 21.39 seconds, analysis
43.44 seconds.

### Growth and fade coverage

The two remaining players had three and one confirmed smoke-hit events respectively,
all excluded by the estimator's blanket 1.5–17 second age window. Directional
sampling now weights reconstructed cell density using the existing native lifetime
curve's lower bound over the recorded fire interval. The occupied-cell threshold
is unchanged. Growing/fading volumes no longer invalidate every shot merely by
age, while missing journal data, packet gaps, HE disturbance and obstruction
checks remain in force. This is still an estimated directional hit rate, not
rendered opacity or an exact server classification.

`directional_samples_include_dense_growth_and_fade` reproduced the exclusion
before the change and passes afterwards. The existing interval-versus-point
native-query test also covers the shared lifetime bounds. Scoring version
`20-smoke-lifetime` and smoke rule version `4-lifetime-weighted-smoke-samples`
prevent reuse of the earlier restricted denominator.
The reported match now has 226 eligible smoke shots across all ten players; the
two previously empty denominators contain three and one shots. Confirmed hits
outside those eligible samples still do not inflate the numerator. Core validation:
296 passed, 6 ignored.
Desktop-path cache reuse, forced-result equality and replay-preservation checks
passed again. Preparation took 21.37 seconds; the separate 30-second analysis gate
still failed at 45.99 seconds (43.76 seconds on the warm run).
