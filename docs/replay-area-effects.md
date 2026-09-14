# Replay area effects

## Fire

Replay schema 5 records active `CInferno.m_firePositions[64]` entries whose
`m_bFireIsBurning[64]` flag is true. Each sampled frame stores world XYZ cells
in `f`; an omitted array means no active cells, not missing data. Geometry follows
the recorded inferno instead of inferring grenade type from the thrower's team.
This naturally preserves differences in spread, terrain and extinction.

The renderer uses a 21-unit footprint per cell (half the default 42-unit flame
spacing). This is a visualization of recorded positions, **not an exact damage
boundary**. The old 150-unit circle is only used for older replay schemas.

`cargo run -p demodesk-core --example effect_props -- <demo>` inspects the
networked fields and sampled entity data without modifying the source demo.

## Smoke

Replay schema 6 uses the shared `smoke` module's reconstructed density instead
of a fixed circle when coverage is available. `ShotCoverage` supplies analysis
queries; `projection::Coverage` supplies top-down coverage independently of the
CS2 wire adapter in `source`.

The adapter streams journal changes without retaining pose history. Projection
samples every 8 ticks and stores only changed snapshots, with horizontal runs
of 20-unit cells and 16 density levels. Seeking uses a binary lookup. `null`
means unavailable and retains the circle fallback; `[]` means no smoke.

Coverage is a top-down estimate, not POV visibility. HE disturbances reuse the
shared spatial falloff with approximate proximity registration; scene occlusion
and client rendering can differ. Shot traces remain visible, but unverified
bullet endpoints are not used to carve smoke openings. The projection interface
accepts qualified bullet effects when available.
