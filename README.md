# MixingSound

Desktop multitrack mixing app for musicians. Rust audio engine + Tauri 2 shell + Svelte 5 UI. Windows first.

## Features (P0 + P1)

- Import WAV / MP3 / FLAC — one track per file, resampled to the project rate
- Synchronized multitrack playback (cpal, lock-free real-time render)
- Per-track gain, pan, mute, solo; master fader; peak/RMS meters with clip latch
- Timeline editing: move clips (also across tracks), trim, split, fade in/out, clip gain, snapping
- Waveform display, undo/redo, save/open projects (`.msproj`)
- Export WAV: 16/24-bit integer (optional dither) or 32-bit float

Not yet: EQ/dynamics/reverb, sends/buses, automation, recording, plugins, LUFS. Sources are decoded fully into RAM.

## Layout

| Path | Content |
|---|---|
| `crates/engine` | Audio engine: model, edits, history, decoding, mixer/render, realtime driver, bounce, project I/O |
| `src-tauri` | Tauri shell: IPC commands, session state, engine status streaming |
| `ui` | Svelte 5 + TypeScript frontend |
| `docs/design/ui-spec.md` | UI specification |

## Develop

Requirements: Rust (stable, edition 2024), Node.js 22+, WebView2 (Windows).

```sh
cd ui && npm ci && cd ..
npx --prefix ui tauri dev        # run the app
cargo test --workspace           # engine + shell tests
cd ui && npm run check && npm run test
```

Build an installer: `npx --prefix ui tauri build` (NSIS).

## Keyboard

Space play/pause · Enter/Home return to start · S split · Delete remove clip · L loop · Ctrl+Z / Ctrl+Shift+Z undo/redo · Ctrl+S save · Ctrl+I import · Ctrl+E export · Ctrl+wheel zoom · Ctrl+M toggle mixer.

## License

MIT
