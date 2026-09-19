# Statistics definitions and limitations

Keep these definitions stable across parser changes. External platforms may use
unpublished filters; do not change attribution merely to match a screenshot.
Cached-result changes require the appropriate schema bump in `store.rs`.

## Damage

ADR is enemy health damage divided by parsed rounds, rounded to one decimal.
Use the same start-to-official-end round windows as kills. Exclude freeze-period,
self, world and friendly damage from enemy totals.

`player_hurt.dmg_health` may include overkill. Count the loss from the victim's
previous remaining health to the event's `health`, capped by raw damage. Track
health per victim and round: entity health can lag multiple hits in one tick and
is only an initial fallback. Excluded hits still update health for later hits.
Utility damage uses the same rule. Event team numbers identify enemies, with
round-roster fallback. Friendly fire is separate and never contributes to ADR.

## Kills, assists and participation

- Openings use the first enemy kill by tick in each round, with stable event order
  for ties. World deaths, suicides, team kills and freeze events do not consume it.
- Validate an assister against the victim independently of the killer. An enemy
  assister can receive credit even when the final death is a team kill.
- Flash assists are credited assists with `assistedflash`, not extra assists.
- KAST counts participating roster rounds with an enemy kill, valid assist,
  survival through official round end, or a death avenged within five seconds in
  the same round. Count each round once; one revenge kill counts once even if it
  avenges multiple victims. Trades describe completed events, not opportunities.
- Clutch records reuse `find_clutches`. Survival is checked at `round_end` and
  does not infer an intent to save. Opponent matrices include only enemy kills;
  their rate is kills / (kills + reverse kills). Missing denominators are unavailable.

## Utility and round trends

Gun shots and grenade throws use `weapon_fire`; knives, utility and C4 do not
count as firearm shots. Count flash, smoke, HE and fire throws separately.

Qualifying blinds last **more than one second** and target a living player.
`player_blind` can report a known-dead target; exclude it. Exactly one second is
excluded, but valid durations between 1.0 and 1.1 seconds must not be lost.
Self-blinds count with teammate blinds. Enemy blind time sums durations without
removing overlaps. These are local definitions, not platform-equivalence claims.

Round trends reuse scoreboard damage/blind rules and official-end windows.
Score difference is A wins minus B wins. Team cash is the sum of player balances
at freeze-end + 1 tick, excluding equipment value. Missing samples remain missing.

## Shot-based aim metrics

Pair enemy hurt events to the last fired shot of the same normalized weapon at
the same or preceding tick within the round. Normalize USP/M4 silencer aliases.
Multiple pellets or penetrations count as one hit shot. Exclude AWP from head-hit
share. Bursts have same-weapon gaps of at most 300 ms; sprays are rifle bursts of
at least three shots. First-shot accuracy uses the first shot of those bursts.
Sample-free rates are unavailable, not zero.

These metrics do not filter by visibility. Collision meshes and radar-spotted
flags cannot establish on-screen visibility: smoke, dynamic objects, hitboxes,
scope FOV and matching map versions still matter. Do not use them as shortcuts for
reaction times, crosshair placement, spotted accuracy or trade opportunities.

## Recoil

The original player trajectory uses `fire_bullets` eye angles (`user_pitch` and
`user_yaw`) and firing origins, not the recoil-bearing firing angles or bullet
impacts. Each burst retains its round, start tick and individual shot samples.
Set a fixed target 10 m ahead of the first view, using 2.54 cm per game unit.
Subtract the pitch/yaw needed to track it from each actual eye origin, then
project the remaining compensation angles onto the standard level 10 m plane.
This normalizes initial orientation, translation and crouch height while
preserving player aim errors. Coordinates are centimetres, right-positive X
and up-positive Y; pulling down is negative. Tracking a different target remains
included, so this is not isolated mouse input or a recoil skill score.

An eligible AK-47, M4A4 or M4A1-S burst starts at recoil index <0.01, contains
at least three shots in one round, has gaps <=300 ms and index increments within
0.05 of one. Missing eye angles/origins, weapon changes and discontinuities
break the burst. Rays without a forward plane intersection remain missing.
Each average point uses only valid samples reaching that shot. Never pad missing shots with zeros. Opposite errors
can cancel in the average, so it cannot replace individual inspection.

The complete negated calibration is projected once into the standard frame.
The reference never depends on player origins, burst length or aim errors.

Parsed schema 20 adds tick-resolution context to per-burst eye angles and origins. Earlier cached
statistics are invalidated and rebuilt through the normal parsing workflow.

The default view selects an individual burst and estimates target-relative spray
movement. One additional tick pass samples the union of burst windows, including
250 ms before each burst. It retains living enemy positions, shooter view angles,
and matching same-weapon hurt/death contacts at the shot tick or one tick later.
Missing positions or duck amounts remain unavailable. Shot origins are measured;
between shots eye height is estimated as `64 - 18 * duck_amount` units. The fixed
torso reference is `48 - 18 * duck_amount` above the pawn origin. These are geometric
proxies, not reconstructed hitboxes or verified body animation.

Target assignment uses recoil-adjusted view direction, a 24-unit body allowance
plus 3 degrees, and 50 ms persistence. Candidates within 0.75 degrees of each other
are ambiguous unless a single matched contact disambiguates them. A single matched
victim with an available live target position takes precedence over the approximate
angular gate; this identifies the target without removing the player's aim error.
Multiple victims hit by one shot do not establish intent. Death alone never establishes transfer:
there must also be a different acquired target afterward. Unknown gaps and transfers
are excluded from the numerical correction estimate.
Original and estimated corrected paths use different colours and independent legend
toggles. Shots lacking an estimate continue their original angular movement while
holding the last established correction offset. The first recovered estimate is
aligned to that continuation; later estimates contribute their changes rather than
snapping back to an absolute position. Tooltips explain the fallback. It never changes the
analysis or enters the corrected average. The displayed corrected path connects
successive points across reaction windows and target changes. Missing correction does not hide shots or
disable playback. Shot state and hit/kill details appear only in point tooltips.

Tracking lag is estimated once per continuous target segment from angular velocities,
using the same observations for all candidate delays from zero through 200 ms.
It requires at least 250 ms of comparable observations, changing target motion,
and a sufficiently strong fit. Constant-speed motion cannot establish lag.
Unidentifiable lag remains unknown, never zero or 200 ms. For these segments,
evaluate each inter-shot target-angle change at every tick delay in 0–200 ms.
For each axis, subtract only the signed component shared by all candidate delays
(the value nearest zero in their range, or zero if the range spans both signs).
Integrate these partial corrections instead of discarding the whole segment.
All candidate histories must be present; reaction intervals and their recovery
boundary retain original movement. Tooltips distinguish partial tracking from
a fitted delay. Fully classified single-target partial tracks may enter averages.
A zero lag for stationary targets requires at least 50 ms of motion
samples below 0.1 degrees/s; empty or shorter tracks do not establish stationarity.
Abrupt motion changes open a reaction interval of at most 200 ms, ending earlier
when a sustained directional response is detected. Reaction shots are retained in
both displayed paths using the held-offset fallback, but omitted from the
numerical correction estimate. This is an operational
tolerance, not a measured human reaction-time claim or per-shot best-fit alignment.

For each engagement, subtract the direction to the target (at the estimated delayed
position, from the current firing origin), then align the first eligible sample
once to that shot's reference. Reacquisition of the same target preserves this
anchor. Switching target changes the anchor but does not reset recoil/shot index.
Consequently the chart shows changes in control, not absolute initial aim error.
Calibration is interpolated between shots for target/lag estimation; actual recoil,
subtick input and animation can differ. This estimator does not verify screen
visibility, smoke, walls or player intent and must not become a skill score.
The corrected average includes only fully classified, uninterrupted single-target
bursts. The original view remains available for every sample.

The fixed reference comes from `src/data/recoil-reference.json`. See the
[calibration SOP](recoil-calibration.md) for capture, evidence and updates.
The old parsed `recoilReference` field is retained for cache compatibility but
ignored; new parses leave it empty. Do not restore per-demo averaging as the
standard: sample composition, firing cadence and subtick aim changes made that
reference vary across matches. `aim_punch_angle` was not available through the
initial event export; requesting a field does not prove it was decoded.

Under the additive angular recoil model, normalized ideal compensation overlaps
this one fixed reference across initial pitch, yaw wrap, movement, crouch
transitions and per-shot averages. This assumes the fixed target and calibrated
recoil model; it does not establish real hit accuracy. Undefined target directions
and corrected rays without forward intersections remain missing.

## Verification

```powershell
cargo test -p demodesk-core --lib
npm run build
npm run test:replay
npm run test:recoil
npm run test:ui
node --experimental-transform-types --test scripts/test-trends.mjs
```

The `parse` and `stat_events` Rust examples accept a local demo and JSON output
path for investigation. `TIMELINE_DEMO_JSON` enables trend endpoint checks against
an existing parse. Keep real demos, exports, player identities and comparisons in
ignored local output directories; committed tests must use synthetic identities.
