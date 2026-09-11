# Recoil calibration SOP

Maintains `src/data/recoil-reference.json`, the fixed angular reference used by
the recoil chart. Reference data is independent of the selected match demo.

The calibration stores firing angles measured with a fixed view. For the player
trajectory calculation, coordinate conventions, averaging and limitations, see
[statistics](statistics.md#recoil).

## Run

Requirements: Windows, Python 3.10+, Rust/Cargo, Steam signed in, and an HLAE
version compatible with the installed CS2. Python scripts use only the standard
library. Close CS2 before running this command from the repository root:

```powershell
python scripts/update-recoil-reference.py --game "D:/SteamLibrary/steamapps/common/Counter-Strike Global Offensive/game/csgo" --weapons m4a1_silencer,ak47,m4a1
```

`--game` points to `game/csgo`. HLAE defaults to
`target/debug/demodesk-data/tools/hlae`; use `--hlae "E:/tools/hlae"` to override it.
A three-weapon run takes approximately 1-2 minutes, excluding initial compilation.

`--weapons` accepts comma- or space-separated internal names, ignores case and
removes duplicates. Omit it to capture every weapon in `scripts/recoil-weapons.json`.
Unknown names fail before the game starts.

| Internal name | Weapon | Item ID | Mode | Magazine |
| --- | --- | --- | --- | --- |
| `m4a1_silencer` | M4A1-S, silencer attached | 60 | 1 | 20 |
| `ak47` | AK-47 | 7 | 0 | 30 |
| `m4a1` | M4A4 | 16 | 0 | 30 |

The default run captures **one standing magazine per weapon**, with normal spread.
It does not repeat captures automatically. A partial update preserves other
weapons only when their `patchVersion` matches. For a different build, capture
all existing weapons or write a separate candidate:

```powershell
python scripts/update-recoil-reference.py --game "D:/SteamLibrary/steamapps/common/Counter-Strike Global Offensive/game/csgo" --weapons ak47 --output target/recoil-lab/ak47-candidate.json
```

## Capture and isolation

1. Build the existing Rust event exporter and extract the same native window hook
   used by video rendering.
2. Launch a dedicated HLAE/CS2 process with `-insecure`, isolated `USRLOCALCSGO`,
   and localhost netcon. Refuse to take over an existing CS2 process.
3. Enable GOTV before loading `de_dust2`, remove bots, join a team and restart
   the round. Use GOTV rather than POV recordings: the initial POV trial lacked
   the player state required for validation.
4. Delete the previous weapon, wait, then give the next weapon. These operations
   must not share a frame: deferred deletion can also remove the new weapon.
5. Stand still, reset pitch/yaw to zero, reload and wait at least four seconds.
   Hold attack for one complete magazine without compensating for recoil.
6. Export and validate every shot before atomically replacing the reference file.
   Release held controls, stop recording and close the owned game on exit.
   If normal shutdown fails, terminate only the recorded game PID.

The native hook hides CS2 windows and suppresses focus changes and cursor warping
before the game starts. Capture requires an `installed` entry in `window-hook.log`.
`unbindall` removes keyboard/mouse bindings; `m_yaw 0` and `m_pitch 0` disable mouse
view changes. Netcon commands such as `setang`, `+attack` and `+reload` still work.
`volume 0` mutes the game. Settings are read back into `input-isolation.log` before
capture. Other applications and the user's original game configuration are unaffected.

| Setting | Purpose |
| --- | --- |
| `mp_roundtime 60`, `mp_roundtime_defuse 60`, `mp_roundtime_hostage 60` | Long rounds |
| `mp_ignore_round_win_conditions 1` | Prevent round completion during capture |
| `mp_freezetime 0` | No opening freeze period |
| `sv_infinite_ammo 2` | Unlimited reserve ammunition with normal reloads |
| `host_timescale 1` | Preserve firing cadence and recoil recovery timing |
| `bot_kick` | Remove interference |
| `mp_respawn_on_death_t 1`, `mp_respawn_on_death_ct 1` | Allow local respawn; dead-player samples still fail validation |
| `tv_delay 0`, `tv_record_immediate 1` | Avoid delayed or incomplete GOTV output |

Do not pause simulation time: firing, reloading and recoil recovery must advance.

## Validation and interpretation

Reject the recording if any required condition fails:

- Weapon item ID, mode, magazine size and shot spacing match the weapon catalog.
- Recoil index starts at zero and increments per shot; remaining ammunition decreases.
- The player is alive with a stable Steam ID, position and zero eye angles.
- `ducked` and `duck_amount` match the requested stance. `ducking` represents a
  transition and cannot identify an already crouched player by itself.
- The first firing angle is near zero, indicating recovered recoil.
- Angles are finite, every magazine is complete, and all recordings share one
  `patch_version`. Do not drop invalid samples, bridge gaps or pad missing shots.

With eye pitch/yaw fixed at zero, stored calibration coordinates in degrees are
`X = fire_bullets.angles_y` and `Y = fire_bullets.angles_x`. The measurement uses
firing direction, not world-space bullet impacts, so distance and player
translation are not projected into the reference.

A single capture verifies completeness and controlled conditions, not
repeatability. Its `maxDeviationDegrees = 0` is not a repeatability result.
The analyzer can compare multiple historical samples; its 0.01-degree acceptance
threshold is not a guarantee of measurement accuracy.

The player's match trajectory uses eye angles and corrects firing-origin
changes against the assumed 10 m target. It still includes target tracking.
It is not isolated recoil compensation or a skill score. The reference
does not cover moving/jumping fire, interrupted bursts, incomplete recovery or
M4A1-S without its silencer. Matching standing/crouched recoil does not imply
identical spread, accuracy or recovery behavior.

## Evidence and game updates

Each run retains demos, event JSON, console logs, a manifest and candidate data
under `target/recoil-lab/update_<timestamp>/`. CS2 creates recordings in `game/csgo`;
the script copies them into the run directory without deleting the originals.
It does not modify game binaries, VPK files or the user's configuration.

The accepted JSON records the game patch, coordinates, sample counts, source demo
names and SHA-256 hashes. `target/` is untracked: archive the run directory before
committing an update so another maintainer can reproduce the analysis.

After a game update:

1. Update HLAE if needed, then run the command above.
2. Review `git diff -- src/data/recoil-reference.json`: build, coverage and angles.
   Changed angles may be valid; never reshape data to match an old reference.
3. Run `python scripts/test-recoil-lab.py`, `npm run test:recoil` and
   `npm run build`, inspect the chart,
   archive evidence and commit the accepted reference with the normal code review.
4. If stance behavior is suspected to have changed, arrange an explicit comparison
   experiment instead of assuming the original result applies to the new build.

Validation failure preserves the accepted file. Inspect logs and event data for
HLAE incompatibility, spawn failure, changed weapon parameters or parser fields.
Do not relax validation to make a broken capture pass. To roll back, restore the
accepted reference JSON from version control; no match-cache invalidation is needed.

A forced process termination can leave CS2 or `target/recoil-lab/update.lock`.
Confirm that no calibration is running before closing that test game and removing
the stale lock. A normal exit or handled failure closes the owned game automatically.

## Initial validation: 2026-09-10

CS2 demo patch 14180, `de_dust2`, 64 ticks/s. The initial study captured 18 complete
magazines per weapon across standing/crouched, independent recordings and
normal/disabled spread: 54 magazines, 1,440 shots. Corresponding firing angles
matched exactly across these conditions. This justified a shared reference and
the reduced single-standing-capture maintenance procedure.

Source manifests under the initial local `target/recoil-lab/` directory:

- `20260910_031948-manifest.json`
- `20260910_032305-manifest.json`
- `20260910_032940-manifest.json`

The empty `20260910_031529` trial was excluded; same-frame weapon deletion/giving
caused the failure. These historical sample counts are not the current asset's counts.

The final hidden run, `update_20260910_034921`, captured one magazine per weapon
(80 shots). Its coordinates exactly matched the initial study. Hook logs confirmed
window/focus/cursor interception, input logs confirmed zero volume and mouse angle
scales, the reference updated successfully, and the game closed automatically.

## Adding weapons

Add the internal weapon name to `scripts/recoil-weapons.json` with these fields:

| Field | Meaning |
| --- | --- |
| Key | Name used by `give weapon_<key>` |
| `itemId` | Event weapon ID |
| `magazine` | Complete magazine size for the selected mode |
| `fireSeconds` | Time sufficient to empty the magazine while holding attack |
| `mode` | Expected firing mode |
| `tickGap` | Allowed adjacent-shot tick interval at 64 ticks/s |

Validate the new weapon through `--weapons <internal_name> --output <candidate>`
before updating the accepted file. The capture assumes holding attack empties the
magazine. Semi-automatic, bolt-action, charge or special modes need appropriate
trigger handling and validation before they are supported. Additional reference
data does not automatically add weapon panels to the product UI.

## Tools

- `scripts/update-recoil-reference.py`: normal entry point, game ownership and publication.
- `scripts/capture-recoil-lab.py`: recording against an existing test netcon session.
- `crates/demodesk-core/examples/recoil_lab.rs`: event export using the existing parser.
- `scripts/analyze-recoil-lab.py`: sample validation and candidate generation.
- `scripts/test-recoil-lab.py`: regression checks without launching CS2.
