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

## Smoke: unresolved visible-volume reconstruction

Observed demos contain `m_VoxelFrameData`, `m_nVoxelFrameDataSize`, and detonation-origin fields. Their presence does not mean the
rendered cloud can be recovered with the current parser.

The [cs2parser format investigation](https://github.com/osztenkurden/cs2parser/blob/master/src/helpers/smokeVoxel.ts)
decodes journal occupancy as **seed voxels**. Density/state and the client's
subsequent volume growth are not fully decoded. Drawing the seed set as the full
smoke would misrepresent visibility. No external implementation was copied.

For now smoke retains the existing 144-unit radius approximation. It cannot be
used to determine whether a player can see through a gap. Calibrating a larger
approximate footprint requires matching in-game views; replacing it with exact
geometry requires the remaining volume-growth/density decoding. No arbitrary
radius increase has been applied.

Sources:
- [Recorded CInferno fields](https://docs.cssharp.dev/api/CounterStrikeSharp.API.Core.CInferno.html)
- [Game convars, including flame spacing](https://cs2.poggu.me/dumped-data/convar-list/)
- [Valve: smoke expands to fill spaces](https://www.counter-strike.net/cs2)
