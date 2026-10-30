# MixingSound UI spec — P0 + P1 ("Warm Console")

Audience: 2–3 implementers who must not have to guess. Backend contract = `ui/src/lib/types.ts`, `api.ts`, `src-tauri/src/commands.rs`, `crates/engine/src/edit.rs`. Design tokens and shared classes already exist: `ui/src/styles/tokens.css`, `ui/src/app.css` (use `.btn`, `.btn-icon`, `.ms`, `.field`, `.seg`, `.lcd`, `.dialog`, `.toast`, `.progress`, `.dropzone`, `.menu`, `.chip`, `.sr-only`). **No raw colours/px for colour, font, radius, motion in components — use the variables.**

## 0. Decisions in one table

| Topic | Decision |
|---|---|
| Look | Warm analog console: graphite surfaces, amber light, LED-ladder meters, DIN-like (Bahnschrift) labels and numerals, Cascadia Mono LCD for time |
| Theme | Dark only (long sessions, dim rooms, waveform/meter colour language). No light theme in P0/P1 |
| Fonts | System only (CSP): `--font-ui` Segoe UI Variable/Segoe UI, `--font-label` Bahnschrift, `--font-mono` Cascadia Mono/Consolas. No `@font-face`, no CDN |
| Window | ≥ 1024×640; default 1440×900. No responsive reflow; below 1024 never happens |
| Units | Everything in engine **frames** at `project.sampleRate`; px only in geometry helpers. Display time `mm:ss.mmm` (`h:mm:ss.mmm` ≥ 1 h) |
| Edits | Clip drag/trim/fade/gain = **ghost-only during the gesture, ONE `applyEdit` on pointer-up, then `endGesture`**. Faders/knobs = **live** (throttled, one IPC in flight) + `endGesture` on release |
| Multi-select | None in P1. One selected clip and/or one selected track |
| Missing in backend | No tempo → no bars/beats. No clip names (use `source.name`). `applyEdit` returns a snapshot only (no created id → diff ids to find a split's right half). `stop` rewind semantics are the engine's; the UI trusts `engine-status.position` |

## 1. Visual direction (10 lines)

1. Surfaces step `--bg-0 → --bg-5` warm graphite (#0f0e0c … #433b32); raised things get a 1px top highlight (`--line-hi`), wells (meters, LCD, inputs) are darker and inset.
2. One accent: amber `--accent` #e8a03c = "on / selected / loop"; play = green `--play`; mute = red-orange; solo = yellow; focus ring = cyan `--focus` (only cool colour → unmistakable on every track colour).
3. Track colours come from `track.color` (engine palette, 8 hues, muted/earthy). Clip header = solid track colour with `--clip-ink` text (≥ 4.5:1 on all 8); body = track colour at ~22% over lane; waveform = track colour lightened.
4. Meters are segmented LED ladders (3px lit segments, 1px gap): green ≤ −18 dBFS, amber −18…−6, orange −6…−1, red > −1 and the clip latch.
5. Typography has two voices: Segoe UI for names/sentences, Bahnschrift CAPS (11px, +0.07em) for labels and all numerals (tabular).
6. Time display is an amber-on-black LCD with faint glow; the only glowing element besides latched buttons.
7. Radii are small on working surfaces (2–3px), 6px only on floating things (dialog, menu, toast). Not uniform on purpose.
8. Rhythm: tight (4–6px) inside groups, 12–16px between groups, hairline dividers between toolbar groups.
9. Depth by layering: lanes (flat) < track headers/mixer (raised) < playhead/loop/ghost (overlay) < menus < modal scrim.
10. Motion only for state feedback (80–240 ms, opacity/transform). Meters animate by ballistics, not CSS. `prefers-reduced-motion` zeroes UI motion.

## 2. Layout (exact)

### 2.1 Shell (WP-A `App.svelte`)

`.shell { display:grid; height:100vh; grid-template-rows: var(--transport-h) minmax(0,1fr) auto var(--statusbar-h); }`

```
┌ Transport 48 ───────────────────────────────────────────────────────────────┐
├ Timeline (flex) ────────────────────────────────────────────────────────────┤
│ corner 208×28 │ Ruler 28                                                     │
│ Track headers │ Lane canvas (vertical scroll shared)                         │
│ 208 wide      │                                                              │
│               │ HScroll 12                                                   │
├ Mixer bar 24 (title, toggle, resize grip) ──────────────────────────────────┤
│ Mixer body 200 (176–340): strips 88 each, horizontal scroll │ MASTER 112     │
├ Status bar 28 ──────────────────────────────────────────────────────────────┤
```
Budget at 640 px window (≈600 content): 600 − 48 − 28 − 224 (mixer) = 300 → ruler 28 + hscroll 12 → 260 = 3.6 lanes. Collapsing the mixer (bar only, 24) gives 7+ lanes.

### 2.2 Transport bar (48px, `--bg-1`, padding 0 12px, items centred, gap 4px; groups split by `.divider-v`)

Left→right (all `.btn.btn-icon` 32×32 unless noted; icons 18px stroke):

| Group | Items (tooltip `title` = "Name (shortcut)") |
|---|---|
| File | New (Ctrl+N), Open… (Ctrl+O), Save (Ctrl+S), Import… (Ctrl+I), Export… (Ctrl+E, **primary style** when project has clips) |
| History | Undo (Ctrl+Z), Redo (Ctrl+Shift+Z / Ctrl+Y) — disabled via `canUndo/canRedo` |
| Transport | Return to start (Home/Enter), **Play/Pause 44×32** (Space; `.is-playing` green while playing, icon swaps), Stop, Loop (L; `.is-on`) |
| Position | `.lcd` 152×36, `mm:ss.mmm`; `title` = `Frame 1,234,567 @ 48 kHz`; double-click → inline `.field` to type a time, Enter seeks, Esc cancels |
| Zoom | Zoom out (Ctrl+−), range slider 72px (log scale, see §5.1), Zoom in (Ctrl+=), Fit (Ctrl+0) |
| spacer | `flex:1` |
| Project | project file stem (or "Untitled") 13px, `.truncate`, max 220px; `.dirty-dot` 8px after it when `snapshot.dirty`; `title` = full path |

Disabled rules: Play/Pause/Stop/Return disabled when `deviceName == null` (title "No audio device"); Save disabled when not dirty **and** path exists; Export disabled when the project has no clips. Width check at 1024: ≈ 776 px used + 224 for the project name.
Button states: rest `--bg-3`; hover `--bg-4`; active (pressed) inset + 0.5px down; focus-visible cyan ring; disabled 55% opacity; latched `.is-on` amber face, dark ink, glow.

### 2.3 Track header column (WP-B, 208px wide, rows exactly `--lane-h` = 72px, 1px bottom hairline)

```
▌ Vocals Lead                       ⋯      row 1 (h 24): name 13px semibold, ⋯ menu btn 24×24
▌ [M][S]  −6.0 dB · L 32                   row 2 (h 24): M,S = .ms 24×22; summary 11px --text-muted
▌                                          row 3: spare 12px (keeps 8px rhythm)
```
- Left colour bar 4px = `track.color` (6px and bg `--bg-3` when selected). Header bg `--bg-2`, hover `--bg-3`.
- Name: double-click or F2 → inline `.field` (Enter commit `renameTrack`, Esc cancel, blur commits; empty → revert).
- `⋯` menu (`.menu`): Rename, Colour (8 swatches 16px, current ringed, `setTrackColor`), Move up / Move down (`moveTrack`), separator, Delete track (`.btn-danger`, `removeTrack`; toast "Track removed – Ctrl+Z to undo").
- Corner cell above headers (208×28, `--bg-1`): `.btn` "＋ Track" (`addTrack` name `Track N`, N = count+1). Also shown at the bottom of the header column as a 72px dashed row when tracks exist.

### 2.4 Ruler (28px, `--bg-1`, canvas)

- Bottom 10px strip is the **loop band** (hit area for loop edit); upper 18px: ticks and labels (Bahnschrift 10px, `--text-muted`). Major tick 10px tall, minor 5px.
- Tick step chosen from `[1,2,5]×10^n` seconds so labels ≥ 70 px apart; minor = step/5 (or /2 for step 5). Labels `m:ss` (step ≥ 1 s) or `m:ss.mmm` (< 1 s).
- Playhead head: 11×9 px triangle `--select` pointing down at the line; draggable (scrub).
- Loop band: enabled = `--accent-wash` fill + 2px `--accent` top edge + 6px end handles; disabled (`loopEnabled=false`) = same shape 100% desaturated, hatched.

### 2.5 Lanes & clips (WP-B, one canvas)

- Lane height 72, alternating `--lane-a/--lane-b`, hairline `--line` between lanes. Clip rect: top = lane+4, height 64, radius 3.
- Clip: header strip 16px (name left, `Bahnschrift` 11px semibold `--clip-ink`; gain badge right `+3.0 dB` only when ≠ 0; hide text if clip < 40 px wide); body: waveform (§5.3) centred in remaining 48px, centre line 1px `rgba(255,255,255,.12)`.
- Fade overlay: darkened (black 40%) triangle on the attenuated side + 1px line; fade handle = 8×8 square at top-left/right of the body (below header) at `x = edge ± fade/fpp`; visible on hover or selected clip.
- States: rest 1px outline `rgba(255,255,255,.08)`; hover outline `.25`; **selected** 2px inset `--select` + header +12% lightness; **muted track** whole lane's clips 40% opacity; **solo-excluded** (some track soloed, this one not) 55%; **missing source** (`missingSources`) diagonal hatch fill + header text "MISSING", waveform not drawn; **dragging** original stays at 35% opacity, ghost drawn full with `--select` outline.
- Playhead: 1px `--select` line full height, DOM element (`transform: translateX`, `will-change: transform`) above the canvas, `pointer-events:none`, `z-index: var(--z-overlay)`.
- Loop region tint over lanes: `--accent-wash` at 50%.
- Snap guide: 1px dashed `--accent-hi` vertical line full height while snapping.
- Bottom HScroll: 12px high track `--bg-0`, thumb `--bg-5` (hover `--accent-lo`), radius pill. Extent = max(projectEnd + 30 s, viewEnd).

### 2.6 Mixer (WP-C)

- **Bar 24px** (`--bg-1`): "MIXER" `.label-caps`, chevron toggle button (Ctrl+M, `aria-expanded`), 6px-tall invisible resize grip on its top edge (cursor `ns-resize`; drag changes `view.mixerHeight` 176–340 clamped to ≤ 55% window height; double-click resets to 200).
- **Body** `--bg-0` inset; channel strips in a horizontally scrolling flex row, **master pinned right** (separated by 1px `--line-strong` and a 12px left shadow).
- **Channel strip** 88 × body-height, bg `--bg-2`, 1px right hairline, padding 6:
  - 3px colour bar (track colour; selected strip: bar 5px + bg `--bg-3`).
  - Name 12px `.truncate`, centred, `title` full name (h 18).
  - Row (h 38): pan knob 36px (left) + stacked `M` / `S` `.ms` 24×18 (right).
  - Fader block (flex): scale labels (18px wide, Bahnschrift 10px) | slot+cap (28px) | stereo meter (L+R, 8px each + 2px gap = 18px). Clip LED 18×6 on top of meter; peak-hold number (11px mono, 30px wide) above it.
  - Gain input (h 22): `.field num` 100% width, shows `−6.0` (or `−∞`); Enter commits, accepts `-inf`, clamps −96…+12.
- **Master strip** 112 wide: label "MASTER" (no pan, no M/S), fader + wider meters (12px each), clip LED 26×8, gain input. `setMasterGain`.
- Fader cap 28×16, radius 2, gradient `--bg-5 → --bg-3`, 2px amber centre line; slot 4px wide `--well`; 0 dB tick drawn as longer line. Cap travel keeps full cap inside slot.
- Pan knob: 36px circle gradient `--bg-4 → --bg-2`, 1px `--line-strong`, indicator line amber 2px; 270° sweep (−135°…+135°), centre detent mark at top; readout under on hover/drag only (tooltip `title`: `L 32` / `C` / `R 18`).

### 2.7 Status bar (28px, `--bg-1`, top hairline, text 12px)

Left: **clip inspector** when a clip is selected — `Source name` (truncate) · `Gain` `.field num` 64px (dB) · `Fade in` / `Fade out` `.field num` 56px (ms) · Start / Length read-only mono · a small `0 dB` reset button. When nothing selected: hint text `--text-muted`: "Space play · S split · Ctrl+wheel zoom". Right: warning chip (`N missing files`, click → toast list), device `.chip` (green dot + device name + `48 kHz`, or red dot "No audio device"), engine error chip (red, `status.error`).

### 2.8 Empty state (no tracks)

Timeline body is replaced by a centred `.dropzone` 520×220: 48px import glyph, "Drop audio files here" (20px), "WAV · MP3 · FLAC — one track per file" (`--text-muted`), `.btn-primary` "Import audio…  Ctrl+I". Mixer shows master strip only. While an OS drag hovers the window (any state): full-window `.dropzone.is-over` overlay (`z-index: var(--z-modal)`, scrim) with "Drop to import N file(s)".

### 2.9 Toasts

Bottom-right stack above the status bar, max 4 (oldest dropped), 380px wide, `role="status"` (errors `role="alert"`). Kinds: info (cyan bar, 4 s), warn (amber, 7 s), error (red, sticky until dismissed or 12 s hover-paused). Close `×` button 24×24. Dedupe identical text within 2 s.

### 2.10 Export dialog (WP-A, `.dialog` 480px, `role="dialog" aria-modal`, focus trapped, Esc closes unless running)

States:
1. **Form**: *Range* `.seg` group [Whole project | Loop region] (loop disabled + hint "Set a loop region on the ruler" when none); *Bit depth* `.seg` [16-bit | 24-bit | 32-bit float]; *Dither* checkbox (enabled for 16/24, default on for 16-bit, off for 24; for float: disabled, unchecked, hint "Not applicable to float"); summary line `Length 03:24.500 · 48 kHz`. Footer: Cancel, **Export…** (primary → native `save()` dialog, default `<project>.wav`, filter wav → run).
2. **Running**: title "Exporting…", `.progress` (set `--p` from `export-progress`), percent text, footer button disabled (no cancel API). Window close/Esc ignored.
3. **Done**: green check + "Export complete"; file path (middle-truncated, `title` full); rows: *Length* `mm:ss.mmm`, *Peak* `−1.8 dBFS` (green ≤ −1, amber −1…0, red > 0), *Clipped samples* `0` (red + sentence "N samples clipped — lower the master fader or export 32-bit float" when > 0). Footer: Export again, Close (primary).
4. **Error**: red text `errorMessage`, footer Back, Close. Also raises an error toast.

## 3. Interactions

### 3.1 Pointer — timeline

| Target | Gesture | Result |
|---|---|---|
| Ruler (not band, not head) | click (< 4 px move) | `seek(frame)` |
| Ruler | press-drag ≥ 4 px | live loop band; on release if length ≥ 50 ms → `setLoopRegion` + enable; Alt = no snap |
| Loop band end handle | drag | resize region; same release rule |
| Loop band | double-click | `clearLoop()` |
| Playhead head | drag | continuous `seek` (rAF-throttled) |
| Empty lane area | click | deselect clip, select that track, `seek` to x |
| Clip body | click | select clip + its track |
| Clip body | drag | move (horizontal + across tracks); ghost; bubble `Start 00:12.345 (+0.250 s)`; on up → `moveClip` + `endGesture` |
| Clip left/right edge (6 px inside edge; ¼ of width if clip < 24 px) | drag, cursor `ew-resize` | trim; left: `start'=s, offset'=offset+(s−start), length'=length−(s−start)`; right: `length'`; clamps: `offset' ≥ 0`, `length' ≥ MIN_CLIP_FRAMES (64)`, `offset'+length' ≤ source.frames`; fades re-clamped by engine |
| Fade handle (hit 8 px) | drag horizontally, cursor `col-resize` | `setClipFades`; clamp `fadeIn+fadeOut ≤ length`; bubble shows ms |
| Clip body | **Ctrl+drag vertical** | clip gain: `dB = start − dy·0.1` (Shift ×0.1), clamp −96…+24, bubble `+3.0 dB`, waveform rescales live; up → `setClipGain` + `endGesture` |
| Clip body | **Shift+click** | split at click x (snaps to playhead within 8 px), cursor scissors; `splitClip` |
| Lane/clip | **Ctrl+wheel** | zoom around mouse (anchor frame stays under cursor), ×/÷ 1.25 per notch |
| Lanes | wheel | vertical scroll; **Shift+wheel** or `deltaX` → horizontal scroll (`scrollFrames += delta·fpp`) |
| Ruler | wheel | horizontal scroll (Ctrl+wheel zooms) |
| Track header | click / double-click name | select track / rename |

**Snapping** (move + trim + loop + split): targets = every other clip's start and end (all tracks), playhead, loop start/end, frame 0. Threshold 8 px (`snapFrames = 8·fpp`). Nearest wins; **Alt held = no snap**. When snapped, draw the guide line. Moving snaps the clip's start *or* end, whichever is closer.
**Esc** during a drag cancels it (no edit sent). Pointer capture on the canvas; pointer-up outside the window still commits.
**Cursors**: default arrow; clip body `grab` (dragging `grabbing`); edges `ew-resize`; fade handle `col-resize`; Shift hover `crosshair`+scissors; Ctrl hover `ns-resize`; ruler `pointer`; loop handles `ew-resize`.

### 3.2 Pointer — mixer

| Control | Gesture | Result |
|---|---|---|
| Fader | drag cap (relative, pointer capture) | live `setTrackGain`/`setMasterGain`; **Shift = 0.1× sensitivity** |
| Fader | click on slot | jump cap there, then drag continues |
| Fader / knob | double-click | reset 0 dB / centre |
| Fader / knob | pointer-up | `endGesture` |
| Knob | vertical drag, 150 px = full −1…1; Shift = 0.1× | live `setTrackPan` |
| Knob | wheel | ±0.02 per notch (Shift ±0.005); gesture ends 400 ms after last notch |
| M / S | click | `setTrackMute`/`setTrackSolo` toggle |
| Clip LED | click | clears that latch |
| Strip name | click | select track (scrolls it into view in the timeline) |

Live edits are sent through `store.liveEdit(key, edit)` (§6): at most one IPC in flight per key, newest value wins, final value always sent before `endGesture`.

### 3.3 Keyboard (global, in `keymap.ts`; ignored while focus is in `input/textarea/select/[contenteditable]`, except Ctrl combos and Esc)

| Key | Action | | Key | Action |
|---|---|---|---|---|
| Space | play / pause | | S | split selected clip at playhead (no selection → clip under playhead on selected track) |
| Enter, Home | return to start | | Delete, Backspace | delete selected clip |
| Esc | cancel gesture / close dialog / deselect | | L | toggle loop (no region → loop selected clip, else 4 s from playhead) |
| Ctrl+Z | undo | | Ctrl+Shift+Z, Ctrl+Y | redo |
| Ctrl+N / O / S | new / open / save | | Ctrl+Shift+S | save as |
| Ctrl+I | import | | Ctrl+E | export dialog |
| Ctrl+= / Ctrl+− / Ctrl+0 | zoom in / out / fit | | Ctrl+M | toggle mixer |
| F | toggle follow playhead | | ← → (canvas focused, clip selected) | nudge 10 ms (Shift 100 ms, Alt 1 frame) |
| [ / ] (canvas focused) | previous / next clip on track | | Alt+↑/↓ (clip selected) | move clip to previous/next track |
| F2 (header focused) | rename track | | Ctrl+Shift+Delete | remove selected track (confirm) |

Browser-default shortcuts (Ctrl+wheel zoom, Ctrl+S, F5, Ctrl+P…) are `preventDefault`ed by the same handler. Nudge/Alt-move send a single `moveClip` per key press (coalesce key `MoveClip` covers repeat; `endGesture` on keyup).

### 3.4 File drag-and-drop & dialogs

- `getCurrentWebview().onDragDropEvent` (WP-A `lib/os.ts`): `enter` → `store.dropActive=true` + count; `leave` → false; `drop` → false, filter paths by extension (case-insens.) `.wav .mp3 .flac` → `importPaths`; a single `.msproj` → `openPath` (with dirty confirm); anything else → warn toast "Skipped N unsupported file(s)".
- `@tauri-apps/plugin-dialog`: `open({multiple:true, filters:[{name:'Audio', extensions:['wav','mp3','flac']}]})`, `open({filters:[{name:'MixingSound project', extensions:['msproj']}]})`, `save(...)`. Permissions already present (`dialog:allow-open/save`). **No native `ask/confirm`** (not permitted) — use the in-app `ConfirmDialog`.
- Dirty guard (New / Open / drop of `.msproj`): ConfirmDialog "Discard unsaved changes?" [Cancel] [Discard] [Save first].
- Long operations (import/open/export) set `store.busy` → thin 2px indeterminate bar under the transport + `aria-busy`; UI stays responsive.

## 4. dB, position and ballistics (pure helpers — exact numbers)

### 4.1 Fader law (`dbScale.ts`) — piecewise linear, position 0 (bottom) … 1 (top)

| dB | +12 | +6 | 0 | −10 | −20 | −30 | −40 | −60 | −96 (−∞) |
|---|---|---|---|---|---|---|---|---|---|
| pos | 1.00 | 0.875 | 0.75 | 0.58 | 0.42 | 0.29 | 0.19 | 0.07 | 0.00 |

`faderPosToDb(pos)`: interpolate; `pos ≤ 0.005` → `MIN_GAIN_DB` (−96, displayed `−∞`). `dbToFaderPos(db)`: `db ≤ −90` → 0. Scale labels: `+12 +6 0 −10 −20 −30 −40 −60 −∞` (omit labels closer than 14 px).

### 4.2 Meter law — range −60…+3 dBFS, piecewise linear, 0 = bottom, 1 = top

| dBFS | −60 | −40 | −30 | −20 | −10 | −6 | 0 | +3 |
|---|---|---|---|---|---|---|---|---|
| pos | 0 | 0.12 | 0.24 | 0.42 | 0.64 | 0.76 | 0.92 | 1.00 |

`linearToDb(x) = x <= 1e-6 ? −120 : 20·log10(x)`. Colour of a lit segment = by its own dBFS: `≤ −18` ok, `≤ −6` warn, `≤ −1` hot, else clip. Segments: height 3 + gap 1; count = floor(meterHeight/4).

### 4.3 Ballistics (`ballistics.ts`) — input arrives at ~30 Hz, draw at rAF

```
step(state, inputDb, dt):                       // dt seconds since last frame
  peak = max(inputDb, state.peak − 24·dt)        // instant attack, 24 dB/s fall
  rms  = max(inputDb_rms, state.rms − 12·dt)     // smoother fall
  hold: if inputDb ≥ state.hold → hold=inputDb, age=0
        else age += dt; if age > 1.5 → hold = max(inputDb, hold − 24·dt)
  clipLatched ||= inputPeakLinear ≥ 0.9999       // cleared only by user click / new project
```
Peak ladder lit to `peak`; RMS drawn as a 2px solid bar down the channel centre (`--text` at 55%); hold tick = 2px `--meter-peak` line. Floor −120 dB; values < −60 draw nothing. Stereo = two channels L/R. `reduced-motion` does not change ballistics.

### 4.4 Time & number formatting (`format.ts`)

`formatTime(frames, sr)`: `m:ss.mmm` zero-padded (`00:12.345`), `h:mm:ss.mmm` ≥ 3600 s. `formatFrames` thousands-separated. `formatDb(db)`: `db ≤ −90` → `−∞`, else one decimal with U+2212 minus (`−6.0`, `+3.0`, `0.0`). `formatPan(p)`: `C` | `L 32` | `R 18` (round(|p|·100)). `parseDb("−inf" | "-6" | "-6.5db")`. Round-trip tested.

## 5. Timeline math & rendering (WP-B)

### 5.1 Geometry (`geometry.ts`)

State: `fpp` (frames per pixel, float, clamp **[1, sampleRate·4]**), `scrollFrames` (left edge, ≥ 0). `frameToX(f) = (f − scrollFrames)/fpp`; `xToFrame(x) = scrollFrames + x·fpp`. Default `fpp = sampleRate/100` (100 px/s). Zoom around anchor: `zoomAt(fpp, scroll, factor, anchorX)` keeps `xToFrame(anchorX)` fixed. Fit: `fpp = (projectEnd·1.05)/viewportW` with min 1 s span. Slider position `t∈[0,1]` ↔ `fpp = exp(lerp(ln fppMin, ln fppMax, 1−t))`. Visible range `[scroll, scroll+W·fpp]`. All helpers are pure and take plain numbers (no DOM).

### 5.2 Canvas lifecycle

One `<canvas>` for all lanes, `position: sticky; top:0`, size = scroll container client size × `devicePixelRatio`, redrawn **only when dirty** (store change, scroll, zoom, resize, hover/drag, tile arrival) via a single `requestAnimationFrame`. Headers are normal DOM rows in the same vertical scroll container (so native scrolling keeps them aligned); canvas reads `scrollTop`. Virtualize: draw only lanes intersecting `[scrollTop, scrollTop+H]` and clips intersecting the visible frame range. Budget: ≤ 4 ms per redraw with 16 visible tracks.

### 5.3 Waveform tiles (`waveformCache.ts`, injected fetcher `(sourceId,start,end,cols)=>Promise<WaveformColumns>`)

- Quantize zoom: `k = round(4·log2(fpp))`, `fppQ = max(1, round(2^(k/4)))`. Tile = `512` columns covering `span = 512·fppQ` source frames; tile `i` = source frames `[i·span, (i+1)·span)`; key `epoch:sourceId:fppQ:i`.
- Needed tiles = those overlapping `[offset, offset+length)` ∩ visible. Request missing ones nearest the viewport centre first, **max 4 in flight**, never duplicate in-flight keys, LRU cap 400 tiles. `projectEpoch` (changes on new/open) is part of the key.
- Draw per screen pixel: map to source frames, take min/max over covering tile columns, scale by clip linear gain; `max>1` or `min<−1` after gain → draw 1px `--meter-clip` at the clipped end. No tile yet → draw only the centre line; on arrival mark dirty (no flicker, no spinner).
- Fades: waveform is NOT scaled by fades; overlay polygon does the job.

### 5.4 Clip edit maths (`clipEdit.ts`, pure, all clamps live here and are unit-tested)

`computeMove`, `computeTrimLeft`, `computeTrimRight`, `computeFades`, `computeGain`, `computeSplit(at strictly inside)`, each returns the resulting clip fields or `null` if invalid, plus `snap(candidates, frame, thresholdFrames, enabled)` → `{frame, target|null}` and `hitTest(...)` returning `{kind:'body'|'edgeL'|'edgeR'|'fadeIn'|'fadeOut', clipId}|null` (priority: fade handles > edges > body; topmost lane clip).

## 6. Frontend architecture

### 6.1 File tree & ownership (disjoint)

```
ui/src/
  main.ts, app.css, styles/tokens.css      (existing; main.ts untouched)
  App.svelte                                         WP-A
  lib/types.ts, api.ts                               (existing, frozen)
  lib/store.svelte.ts  + store.test.ts               WP-A
  lib/format.ts        + format.test.ts              WP-A
  lib/keymap.ts        + keymap.test.ts              WP-A
  lib/os.ts                                          WP-A (dialogs + drag-drop wrappers)
  components/shell/  Transport.svelte, TimeDisplay.svelte, StatusBar.svelte, ClipInspector.svelte,
                     ExportDialog.svelte, ConfirmDialog.svelte, ToastHost.svelte, EmptyState.svelte,
                     DropOverlay.svelte, Icon.svelte                     WP-A
  lib/layout.ts, geometry.ts, snap.ts, clipEdit.ts, hitTest.ts, waveformCache.ts, ruler.ts  + *.test.ts   WP-B
  components/timeline/ Timeline.svelte, TrackHeader.svelte, TrackMenu.svelte, Ruler.svelte,
                       LaneCanvas.svelte, drawClip.ts, Playhead.svelte, HScroll.svelte               WP-B
  lib/dbScale.ts, ballistics.ts, knob.ts, meterBank.ts  + *.test.ts   WP-C
  components/mixer/    MixerPanel.svelte, ChannelStrip.svelte, MasterStrip.svelte, Fader.svelte,
                       Knob.svelte, Meter.svelte                                                        WP-C
```
`Icon.svelte` API: `<Icon name="play" size={18} />`; names: `new open save import export undo redo return play pause stop loop zoom-in zoom-out fit plus more close warn mixer check`. WP-B/C use only `more`, `plus`, `close`, `mixer`, `warn` — controller commits a stub `Icon.svelte` (all names, simple stroke SVG) before the wave so B/C compile.

### 6.2 Store contract (`lib/store.svelte.ts`, WP-A). Controller commits this skeleton first; B and C code against it.

```ts
export type ToastKind = 'info' | 'warn' | 'error'
export interface Toast { id: number; kind: ToastKind; text: string; ttlMs: number | null }
export interface LoopRegion { start: number; end: number }          // frames, start < end
export interface ViewState {
  fpp: number; scrollFrames: number         // §5.1
  viewportW: number; viewportH: number      // set by Timeline via setViewport
  follow: boolean
  mixerOpen: boolean; mixerHeight: number   // 176..340, default 200
}
export interface ExportUi { open: boolean; phase: 'form'|'running'|'done'|'error'; progress: number; report: BounceReport | null; error: string | null }

class AppStore {
  // ---- reactive state (read-only to components; mutate only through actions) ----
  snapshot: AppSnapshot | null            // $state.raw; null until init() resolves
  status: EngineStatus                    // $state.raw, replaced ~30 Hz; initial: idle, zero levels
  position: number                        // frames; = status.position, or optimistic after seek()
  loop: LoopRegion | null; loopEnabled: boolean
  selection: { clipId: number | null; trackId: number | null }
  view: ViewState
  toasts: Toast[]
  busy: null | 'import' | 'open' | 'save' | 'export'
  dropActive: boolean; dropCount: number
  export: ExportUi
  clipLatch: Record<number | 'master', boolean>   // per-track + master clip latches
  projectEpoch: number                    // ++ on new/open (waveform cache key)
  announcement: string                    // aria-live text

  // ---- derived getters ----
  get project(): Project | null; get dirty(): boolean; get canUndo(): boolean; get canRedo(): boolean
  get playing(): boolean; get hasDevice(): boolean; get sampleRate(): number
  get projectEnd(): number                // max clip end over all tracks, frames (0 if none)
  get anySolo(): boolean
  trackById(id): Track | undefined; clipById(id): { clip: Clip; track: Track } | undefined
  sourceById(id): SourceRef | undefined; isMissing(sourceId): boolean
  levelOf(trackId): StereoLevel | undefined        // latest status.tracks entry

  // ---- lifecycle ----
  init(): Promise<void>                   // api.init + subscribe onEngineStatus/onExportProgress + drag-drop
  dispose(): void
  // ---- project ----
  newProject(): Promise<void>; openDialog(): Promise<void>; openPath(p: string): Promise<void>
  save(): Promise<void>; saveAs(): Promise<void>
  importDialog(): Promise<void>; importPaths(paths: string[]): Promise<void>
  undo(): Promise<void>; redo(): Promise<void>
  // ---- transport ----
  play(): Promise<void>; pause(): Promise<void>; togglePlay(): Promise<void>; stop(): Promise<void>
  returnToStart(): Promise<void>; seek(frame: number): Promise<void>   // optimistic: sets position first
  setLoopRegion(start: number, end: number): Promise<void>             // also enables
  toggleLoop(): Promise<void>; clearLoop(): Promise<void>
  // ---- edits (each: api.applyEdit → replace snapshot; error → toast, snapshot unchanged) ----
  edit(e: Edit): Promise<boolean>                                      // one undo step
  liveEdit(key: string, e: Edit): void                                 // throttled: 1 in flight/key, newest wins
  endGesture(): Promise<void>                                          // flush pending liveEdits, then api.endGesture
  addTrack(): Promise<void>; removeTrack(id): Promise<void>; renameTrack(id, name): Promise<void>
  moveTrack(id, index): Promise<void>; setTrackColor(id, color): Promise<void>
  setTrackMute(id, v): Promise<void>; setTrackSolo(id, v): Promise<void>
  setTrackGain(id, db): void; setTrackPan(id, pan): void; setMasterGain(db): void   // live (liveEdit)
  moveClip(clip, track, start): Promise<void>; trimClip(clip, start, offset, length): Promise<void>
  setClipFades(clip, fin, fout): Promise<void>; setClipGain(clip, db): Promise<void>
  splitClip(clip, at): Promise<void>; removeClip(clip): Promise<void>
  splitSelectedAtPlayhead(): Promise<void>; deleteSelection(): Promise<void>
  // ---- selection / view ----
  selectClip(clipId: number | null): void      // also selects its track
  selectTrack(trackId: number | null): void
  setViewport(w: number, h: number): void
  zoomBy(factor: number, anchorPx?: number): void; setFpp(fpp: number): void
  fitProject(): void; scrollToFrame(f: number): void; scrollByPx(dx: number): void   // clamps ≥ 0
  toggleFollow(): void; toggleMixer(): void; setMixerHeight(px: number): void
  clearClip(id: number | 'master'): void
  // ---- export ----
  openExport(): void; closeExport(): void
  runExport(o: { range: 'project' | 'loop'; bitDepth: BitDepth; dither: boolean }): Promise<void>
  // ---- feedback ----
  toast(kind: ToastKind, text: string): void; dismissToast(id: number): void; announce(t: string): void
}
export const store: AppStore
```
Rules: components call `store.x()` (never detach methods). Errors: every action catches, `toast('error', errorMessage(e))`, never throws. Snapshot `warnings` that were not in the previous snapshot become warn toasts. `missingSources` non-empty after open → one warn toast. `status.error` distinct from last → error toast. Follow: while `playing && view.follow`, if playhead leaves `[scroll, scroll+0.9·W·fpp]` → `scrollToFrame(position − 0.1·W·fpp)`; any manual scroll during play sets `follow=false` until next `play()`. `clipLatch` updated on every status: track peak ≥ 0.9999 → latch; master latch also from `status.clipped`; reset on new/open. Playhead/time/meters read `store.position`/`store.status` directly in rAF (no extra reactive layers). Debug `console.*` forbidden.

### 6.3 Work packages

**WP-A — Shell, store, transport, dialogs** (owner: 1 implementer). Files: `App.svelte`, `lib/{store.svelte,format,keymap,os}.ts` (+tests), all `components/shell/*`. Acceptance: store tests with mocked `api` (play/seek optimistic, liveEdit throttle/ordering/flush, warnings diff, error→toast, latch, follow-scroll, export phases); format/keymap tests; Transport all states of §2.2; ExportDialog 4 states; ConfirmDialog; ToastHost; drop overlay; Esc/focus-trap/return-focus; `npm run test && npm run check` clean.

**WP-B — Timeline** (owner: 1). Files: `lib/{layout,geometry,snap,clipEdit,hitTest,waveformCache,ruler}.ts` (+tests), `components/timeline/*`. Acceptance: all gestures of §3.1; helper tests (zoom anchor invariance, fit, snap thresholds/Alt, trim clamps incl. offset ≥ 0 and source end, fade clamp, split strictly inside, hit-test priority, tile math/dedupe/LRU, ruler tick choice); virtualization; 16 tracks × 8 clips scroll/zoom at 60 fps (manual note); keyboard on canvas; a11y live announcements (`store.announce`: "Clip Drums selected on track 2, starts 00:04.000, length 00:08.500").

**WP-C — Mixer** (owner: 1). Files: `lib/{dbScale,ballistics,knob,meterBank}.ts` (+tests), `components/mixer/*`. Acceptance: §2.6/§3.2; fader/meter laws tested at every table point + round-trip + clamping + −∞; ballistics tested (attack, fall rate, hold 1.5 s, latch); knob angle/drag maths; single shared rAF in `meterBank` feeding all `Meter.svelte`; strips virtualised horizontally when > 24 tracks; keyboard (§7).

Integration (controller): `App.svelte` composes `<Transport/> <Timeline/> <MixerPanel/> <StatusBar/> <ToastHost/> <ExportDialog/> <ConfirmDialog/> <DropOverlay/>`; set `<title>MixingSound</title>` in `ui/index.html`; add `lang`/no other change.

## 7. Accessibility

- **Focus**: global `:focus-visible` 2px `--focus` ring, offset 1px (in `app.css`); never remove outlines; dialogs trap focus and restore it to the opener on close; first focus = first field (form) or primary button.
- **Landmarks/roles**: transport `role="toolbar" aria-label="Transport"` (arrow-key roving optional, Tab order = DOM order); timeline canvas `tabindex="0" role="application" aria-label="Timeline"` + `aria-describedby` pointing to a `.sr-only` hint listing its keys; track headers `role="group" aria-label="Track Vocals"`; mixer `role="group" aria-label="Mixer"`; status bar `role="status"` `aria-live="off"`; toasts `aria-live="polite"` (errors `role="alert"`); `store.announcement` rendered in a `.sr-only` `aria-live="polite"` region.
- **Buttons**: every icon-only control has `aria-label` identical to its tooltip name; toggles use `aria-pressed` (loop, mute, solo, follow), mixer toggle `aria-expanded`; Mute/Solo label `Mute Vocals` / `Solo Vocals`.
- **Fader**: `role="slider" tabindex="0" aria-label="Vocals volume" aria-orientation="vertical" aria-valuemin="-96" aria-valuemax="12" aria-valuenow={db} aria-valuetext="−6.0 dB"` (`−∞ dB` when ≤ −90). Keys: ↑/→ +0.5 dB, ↓/← −0.5, **Shift ±0.1**, PgUp/PgDn ±3, Home = −∞, End = +12, `0` = reset 0 dB. Each key press = live edit; `endGesture` on keyup / blur / 400 ms idle.
- **Pan knob**: `role="slider" aria-valuemin="-100" aria-valuemax="100" aria-valuenow aria-valuetext="Left 32" | "Center" | "Right 18"`; ←/↓ −0.02, →/↑ +0.02, Shift ±0.005, PgUp/PgDn ±0.1, Home/End = hard L/R, `0` = centre.
- **Meters**: `role="img"`, `aria-label` = `Vocals level, peak −3.2 dB` refreshed at most 2×/s; peak-hold numeric readout is real text; clip state also by LED shape/label (`CLIP`), never colour alone.
- **Contrast**: `--text` 14.4:1, `--text-2` 7.8:1, `--text-muted` 5.9:1 on `--bg-1` (4.9:1 on `--bg-3`); `--clip-ink` on all 8 track colours ≥ 4.5:1; amber `--accent` on `--bg-1` 8.3:1; `--text-on-accent` on `--accent` 8.4:1. Disabled text is exempt. No information by colour alone (mute `M`, solo `S`, missing-source hatch + word).
- **Pointer targets**: ≥ 24×24 (`--hit-min`); drag handles have ≥ 8 px hit slop.
- **Motion**: `prefers-reduced-motion` removes UI transitions (tokens set durations to 0); playhead/meters are data, still update.

## 8. Visual acceptance checklist (reviewer compares against screenshots at 1024×640 and 1440×900)

1. Transport fits one row at 1024 with project name visible; no horizontal scrollbar anywhere on the page.
2. 3 tracks + 12 clips: lanes alternate, clip headers show source names, selected clip has cream outline, playhead crosses all lanes.
3. Mute a track → its clips dim to 40%; solo another → non-solo tracks dim to 55%.
4. Meters show green→red ladder; play a clipping track → clip LED latches red until clicked; hold tick lingers 1.5 s.
5. Zoom with Ctrl+wheel keeps the frame under the mouse stationary; waveform sharpens without flicker.
6. Empty project shows the dropzone; dragging a file over the window shows the full-window drop overlay.
7. Export dialog renders all four states; peak/clipped values coloured per §2.10.
8. Tab through the whole UI: every control shows the cyan ring; faders adjust with arrows; Esc closes dialogs.
9. No raw hex/px colours or fonts in component CSS (`grep -E '#[0-9a-fA-F]{3,6}' ui/src/components` returns nothing except tokens).

## Change history

| Version | Date | Content | Scope |
|---|---|---|---|
| v1.0 | 2026-10-30 | Initial P0+P1 UI spec | docs/design, ui/src/styles, ui/src/app.css |
