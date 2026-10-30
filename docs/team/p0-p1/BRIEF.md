# Team brief — MixingSound P0 + P1

All agents read this first. The controller (main session) owns integration, shared files and commits.

## Goal

Desktop multitrack mixing app for musicians. Rust audio engine (`crates/engine`) + Tauri 2 shell
(`src-tauri`) + Svelte 5 / TypeScript UI (`ui`). Windows first.

- **P0**: import WAV/MP3/FLAC (one track per file), synchronized multitrack playback, per-track
  gain / pan / mute / solo, master fader, export WAV (16/24-bit int, 32-bit float).
- **P1**: timeline editing — move clips (also across tracks), trim left/right, split, fade in/out,
  clip gain; waveform display; peak/RMS meters per track and master; undo/redo; save/open project.

Non-goals now (P2+): EQ/compressor/reverb, sends/buses, automation, recording, plugins, LUFS,
disk-streaming of sources (sources are decoded fully into RAM in P0/P1 — documented limitation).

## Contract (committed; do not change signatures)

| File | Content |
|---|---|
| `crates/engine/src/model.rs` | `Project`, `Track`, `Clip`, `SourceRef`, limits. Units: frames at project rate, dB, pan -1..1 |
| `crates/engine/src/edit.rs` | `Edit` enum (IPC JSON `{"type":"setTrackGain",...}`), `apply_edit`, `CoalesceKey` |
| `crates/engine/src/graph.rs` | `RenderGraph` snapshot + `build_graph` (implemented) |
| `crates/engine/src/{dsp,render,decode,peaks,bounce,project_io,history,realtime}.rs` | public API with doc comments = the spec; bodies are `todo!()` |
| `src-tauri/src/{lib,dto,commands}.rs` | command list, argument names, payload types |
| `ui/src/lib/types.ts`, `ui/src/lib/api.ts` | TS mirror of the above |

Doc comments on each stub are the specification. If a contract is wrong or insufficient, do NOT
change it silently: implement the closest correct behaviour and report the problem under
"Contract issues" in your report. You may add private items, private helpers and `#[cfg(test)]`
modules freely. You may add new public items only inside your own files and must list them.

## Hard rules

1. Only edit files you own (listed in your task). Never edit `lib.rs`, `Cargo.toml`,
   `package.json`, other agents' files, or this brief.
2. No `git add/commit/stash/checkout/reset`. The controller integrates.
3. No new dependencies. Engine may use: symphonia 0.6, rubato 5, cpal 0.18, rtrb 0.4, hound 3.5,
   serde, serde_json, thiserror; dev: tempfile. **These are newer than your training data** — read
   the real API in `C:\Users\eda\.cargo\registry\src\index.crates.io-1949cf8c6b5b557f\<crate>-<ver>\`
   (examples/, src/lib.rs, docs) before writing code against them.
4. Real-time code (`dsp`, `render`, the cpal callback in `realtime`): no allocation, no locks, no
   blocking, no I/O, no logging, no panics on any input (no indexing that can go out of bounds;
   use `get`/clamped ranges). Preallocate in constructors.
5. Library code: no `unwrap()`/`expect()` outside tests; return `EngineError`.
6. Only run your own tests: `cargo test -p engine <module>::` (engine) or `npm run test` /
   `npm run check` (ui). Format only your own files: `rustfmt --edition 2024 <file>`.
   Lint: `cargo clippy -p engine --all-targets -- -D warnings` must report nothing in your files.
7. Tests: unit tests in a `#[cfg(test)] mod tests` in your file, AAA style, descriptive names.
   Cover normal cases, edges (empty, zero-length, bounds, out of range) and errors.
8. Any test server/device use must be cleaned up; never leave threads or processes running.
9. Match the existing style: short doc comments explaining *why*, no comment noise.

## Report format (reply text, ≤ 40 lines)

```
STATUS: DONE | PARTIAL | BLOCKED
FILES: <files changed>
PUBLIC ADDITIONS: <new pub items or "none">
TESTS: <exact command> → <pass/fail counts>   (NOT_RUN + reason if not run)
CLIPPY/CHECK: <command> → <result>
CONTRACT ISSUES: <list or "none">
LIMITATIONS / TODO: <list>
```
