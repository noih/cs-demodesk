# Demo parser boundary

`parser::DemoParser` is the application facade. It accepts an injected `DemoBackend`;
production selects the vendored demoparser adapter in its constructor.

- `DemoQueries`: header, server clock, supported property names, events and column-major samples.
- `DemoStream`: bounded-memory packet snapshots, smoke replay and lossless match-state output.
- `parser/contract.rs` and `parser/rows.rs`: application-owned values. Steam IDs retain `u64` precision;
  absent samples remain absent; native floats retain their bits.
- `parser/normalize.rs`: gameplay normalization into `DemoData`, independent of upstream structs.
- `parser/demoparser*`: all vendor imports, wire conversions and packet-specific writers.
  The `analysis::compact` and `analysis::scene` modules expose stable app-facing contracts.
- `vendor/demoparser`: upstream engine plus our explicit wire-format extensions. Update this and the
  adapter together; do not replace the vendor directory wholesale and lose pose/smoke extensions.

The small replicated-convar reader is also behind `parser::replicated_convars`. It reads raw packet
metadata independently of entity decoding, so verified spread policy can remain available without
inventing missing entity data.

## November 2026 compatibility fix

CS2 build 14197 introduced the `fixed8` encoder. Reading a byte whose top bit is set as a varint
consumes the next field and desynchronizes entity updates. Depending on the build profile this
surfaced as `IllegalPathOp` or a bit-reader underflow panic.

The fixed8 patch follows upstream commit
[4ba136aa](https://github.com/LaihoE/demoparser/commit/4ba136aa27aa7cf40f0d34b3b36512aa70cfb442).
It preserves signed/unsigned result types and leaves vector lengths as varints.

Unsupported explicit encoders now fail during schema construction with the encoder, field and type.
There is no speculative skip of unknown variable-length data. Exhausted bit reads and invalid path
pops return errors instead of underflowing. These checks improve failure isolation; they cannot
make unknown future formats automatically correct or guarantee every malformed input is panic-free.

## Updating and validating

1. Keep changes in the vendor engine and adapter. Domain analysis and UI must not import vendor types.
2. Run core tests and the parser boundary checks:
   `cargo test -p demodesk-core --lib`
   `node --experimental-transform-types --test scripts/test-parser-boundary.mjs`
3. Exercise the raw decoder regressions with `cargo test -p parser --lib`.
4. Run `cargo run -p demodesk-core --example parse -- <demo.dem> <output.json>` against both failing
   new recordings and known older recordings. Compare normalized JSON with a retained baseline.
   Private demos and generated results belong outside Git (for example under ignored `out/`).
5. Check the desktop crate and frontend. No CS2 launch is needed for parser validation.

Tool error provenance is attached at the component boundary and retained through error codes,
serialization and UI translation. Existing saved errors without provenance remain explicitly
unattributed; tool names are not inferred from their message text.
