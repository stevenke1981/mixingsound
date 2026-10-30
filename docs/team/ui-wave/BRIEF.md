# Team brief — MixingSound UI wave (P0 + P1 frontend)

Read first: this file, `docs/team/p0-p1/BRIEF.md` (hard rules still apply), and **`docs/design/ui-spec.md`
(the spec — follow it exactly)**. Backend is done and frozen: `ui/src/lib/types.ts`, `ui/src/lib/api.ts`.

## Shared contracts (committed; do not change signatures)

| File | Owner of body | Notes |
|---|---|---|
| `ui/src/lib/store.svelte.ts` | WP-A | skeleton = API all others call |
| `ui/src/lib/{geometry,snap,clipEdit,hitTest,ruler,waveformCache}.ts` | WP-B1 | stubs with signatures + doc comments. `layout.ts` from the spec is merged into `geometry.ts` |
| `ui/src/components/shell/Icon.svelte` | WP-A (artwork only) | |
| `ui/src/styles/tokens.css`, `ui/src/app.css` | controller | read-only; use the variables/classes |

## Work packages (spec §6.1/§6.3)

- **WP-A** shell/store: `App.svelte`, `lib/{store.svelte,format,keymap,os}.ts` (+ `*.test.ts`), all of
  `components/shell/*`. App composes `<Transport/> <Timeline/> <MixerPanel/> <StatusBar/> <ToastHost/>
  <ExportDialog/> <ConfirmDialog/> <DropOverlay/>` from `components/timeline/Timeline.svelte` and
  `components/mixer/MixerPanel.svelte` (both take no props; they may not exist in your worktree yet — create
  a 1-line placeholder ONLY if needed to compile and list it in the report; the controller drops it).
- **WP-B1** timeline maths: bodies + tests for `lib/{geometry,snap,clipEdit,hitTest,ruler,waveformCache}.ts`.
- **WP-B2** timeline components: `components/timeline/*` (Timeline, TrackHeader, TrackMenu, Ruler, LaneCanvas,
  drawClip.ts, Playhead, HScroll). Call only the contract signatures of B1 libs and the store.
- **WP-C** mixer: `lib/{dbScale,ballistics,knob,meterBank}.ts` (+ tests), `components/mixer/*`.

## Rules (in addition to p0-p1 BRIEF)

1. Only touch your own files. No git add/commit/stash/checkout. No new npm dependencies.
2. Svelte 5 runes only (`$state`, `$derived`, `$effect`, `$props`); TypeScript strict; no `any`.
3. No raw hex colours / font names in components — CSS variables from tokens.css. Canvas code reads token
   values once via `getComputedStyle(document.documentElement).getPropertyValue('--x')`.
4. No `console.*`. Errors go through `store.toast`.
5. Verify: `cd ui && npm run check` (0 errors, 0 warnings in your files) and `npx vitest run <your tests>`.
   Tests: vitest, AAA, descriptive names, edges + errors. Mock `./api` with `vi.mock` where needed.
6. Report format: same as p0-p1 BRIEF (STATUS/FILES/PUBLIC ADDITIONS/TESTS/CHECK/CONTRACT ISSUES/LIMITATIONS).
