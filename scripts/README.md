# Developer scripts

See [Compatibility diagnostics](../docs/compatibility-diagnostics.md) for the CS2
update scanner, baseline workflow, regression checks, and log retention rules.
See [Background recording](../docs/background-recording.md) for runtime ownership,
hidden-window hooks, and crash behavior.

Run `npm run test:replay` for pure replay-state regressions. It covers the
reported round-9 fake defuses, normal kit/no-kit defuses, explicit aborts,
death, and seeking. Current CS2 demos can omit `bomb_abortdefuse`; sampled
`is_defusing` state is therefore checked even when no abort event arrives.
Replay schema 3 rebuilds earlier cached streams to include that state.