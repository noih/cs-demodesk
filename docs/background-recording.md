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

The protected seek protocol is owned by `render/seek.rs`: `SeekWindow::schedule`
adds the jump, audio reset, preroll and pause boundary together; `RecordingGate`
releases capture only after the wall-clock wait. Rendering and local editing
experiments use this same path. Add new video seek operations through this
protocol rather than sending `demo_gototick` directly. Perspective changes at
the current tick do not seek; overlapping footage is recorded in separate
passes and assembled afterward.


Every clip, including a single-clip export and the first clip of each pass,
seeks three seconds before its recording boundary. After seeking, the schedule
clears deferred sound events (`snd_soundevent_clear_deferred`) and stops active
sound events (`snd_sos_stop_all_soundevents`), then plays normally at speed 1.
At the recording boundary it pauses for three wall-clock seconds before starting
capture. Audio reset/preroll and the wait are included in the job log.
Clips too close together for preroll use separate recording passes. Near the
beginning of a demo, preroll is limited by the earliest usable setup tick; the
three-second wall-clock wait still applies. These commands require in-game
validation after CS2 audio changes; schedule tests cannot prove audible results.

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


## Tool update isolation

HLAE, FFmpeg and Source 2 Viewer downloads require the release asset's SHA-256
`digest`. Missing or mismatched hashes stop installation without changing the old
tool. Verified ZIPs and a pending-install record survive an app crash. On restart,
recovery runs in the background, rechecks the ZIP, discards incomplete extracted
files and extracts again. Staging stays on the target volume, including custom
installation directories. Explicit cancellation or failure removes the pending
workspace so it does not restart automatically.

An update requests exclusive access only after extraction. Existing exports and
analysis/map extraction finish first; new work using that tool waits. Unrelated
tools remain available. The lease covers replacement, settings publication and
the startup probe, and releases on success, error or cancellation. On Windows,
externally running HLAE/CS2, FFmpeg/ffprobe or Source2Viewer-CLI also delay their
corresponding replacement. This process-name check is deliberately conservative;
users may need to close an independently installed copy too. External programs
can still race a launch; filesystem replacement errors preserve/restore the old
installation rather than forcing replacement.

The global progress dialog shows verification, extraction and waiting/replacement
phases, including startup recovery. Closing it leaves the operation running;
Cancel aborts preparation/waiting. Once the short directory swap begins it finishes
or rolls back as one critical section. ZIP validation does not imply runtime
compatibility or establish individual installed-file hashes.

## Utility teamwork editing

The optional `utilityTeamwork` export setting defaults to false for new and saved
jobs. It is exposed by Editing > Utility teamwork for highlight exports. Analysis
exports retain their requested footage/order. Opening and generic combat-view
controls are disabled until implemented; they do not change exports.

`render/utility.rs` parses the selected local demo only when enabled. It correlates
`player_blind`, `player_hurt`, `player_death`, throws and detonation events by player,
victim, team, round and time. Assist flags are not required. Basic support includes
flash blindness, HE damage and fire damage. All recognized grenade damage, including
1 HP impact damage and decoy damage, can qualify; direct grenade deaths do not
require a separate hurt event. Repeated fire damage produces one sequence per kill.
Pure gun-to-gun damage does not produce a teamwork edit. Ambiguous or missing throws,
unknown player slots and unrelated/friendly damage retain the original footage.

Shots are planned in final story order, then captured in forward-only passes using
the common seek/audio gate. Adjacent continuous views are coalesced. Output indices
restore story order before the existing highlight join, merge and size-fitting
pipeline. Repeated demo time is intentional. Main attack footage starts no earlier
than three seconds before the kill, preferably half a second before the effect.
The victim shot targets about one second of effect plus its visual ramp; if death
comes sooner it ends before death and obtains context from before the effect. A
lethal grenade instead keeps the impact/death as the payoff; a preceding teammate's
actual gun damage can receive a shot before that payoff.

The bundled `utility-camera.mjs` uses the thrower's rendered eye direction for the
shoulder shot. A verified flash/HE projectile entity enables a short bridge into
trajectory following; its heading is stable through bounces. Fire-area entities
are not bottle projectile IDs. Fire and rare impact shots retain shoulder footage
when there is no reliable projectile link, rather than following the wrong entity.
The override unregisters at the cut. World collision avoidance is not implemented;
wide camera offsets are deliberately avoided, but map geometry can still occlude
an otherwise eligible shot. The planner is an event heuristic, not a claim that
all damage in a time window caused the eventual kill.

Checks: `cargo test -p demodesk-core render::utility::`,
`node --test scripts/test-utility-camera.mjs`, and the normal build/UI checks.
The opt-in ignored `render::experiments::record_editing_experiment` accepts the
`utility` variant to exercise the actual export path with a local manifest; private
fixtures and resulting videos stay under ignored `out/`.

Opening camera is optional (openingCamera, default false). It prepends one 1.8-second protagonist shot per highlight before utility edits: 1.2 seconds following the sampled eye route, then 0.6 seconds accelerating toward first person, including a 0.125-second spectator handoff guard. The script is bundled from render/opening-camera.mjs and shares the standard seek/audio gate. Existing action windows remain intact. Missing player slots, protagonist death, and insufficient room before the round boundary skip the opening. Route clearance is not a collision query; complex geometry still needs visual validation. Manual ordered clips keep their existing behavior.

Combat perspectives are optional (combatPerspectives, default false). The first selector qualifies enemy gun kills with at least 20 reciprocal gun-damage points received in the preceding 2 seconds; blind exchanges, missing slots and utility-victim shots are excluded. Kills within 3 seconds form a continuous exchange, with the highest incoming-damage candidate replayed after the exchange; ongoing main-player combat delays insertion. There is no per-video replay-count cap. Enemy replays span kill minus 1.5 seconds to kill plus 0.5 seconds, then resume the uncut main-POV aftermath, including round-result metadata. No automatic peek/aim geometry quality claim is made. Shared seek/audio gates protect every replay recording.

Team push is optional (`teamPush`, default false). Before a protagonist kill, the planner
samples positions and current teams at eight samples/second. It requires at least two nearby
living T players to advance in aligned directions over 1.5 seconds; static clusters,
defending teams, missing tracks and round crossings do not qualify. It may extend the
existing window backward by at most four seconds to establish the group, without shortening
the protagonist's action or aftermath. Group shots are 0.5–2 seconds, independently selected
from the lead-in. A nearby teammate's gun death followed by the protagonist killing the same
enemy within four seconds can receive an independent encounter shot, ending 0.5 seconds
after the teammate's death and before protagonist combat. Multi-entry site reasoning and
generic gun co-attacks are not inferred.

Camera search uses installed-map static physics, body framing and separation across the
sampled interval. No candidate, missing tools or preparation failure retains the original
POV; cancellation still aborts the job. Body envelopes and static geometry are approximations:
smoke, dynamic props, corpse poses, historical map versions and visual recognizability are
not fully qualified. The same camera limits established in the recorded experiments apply.
The bounded pan follows subject-centroid motion without random movement. Its projected
margins are checked before playback; failed pan framing retains the fixed view. Only nearby,
contiguous shots with a checked connecting path may use the common transition; POV gaps cut.

Utility and enemy-replay shots take precedence. Team cameras are clipped to the initial
main-POV portions and never applied to later replay returns. An applicable team introduction
suppresses the additional opening-camera introduction. Native handoff guards keep the eye
override briefly after returning to first person, including before clip/round-result boundaries.
Manual ordered clips bypass these edits. All seeks still use the central audio-reset gate.
