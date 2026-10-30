<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import ChannelStrip from './ChannelStrip.svelte'
  import MasterStrip from './MasterStrip.svelte'

  /** Mixer body height limits and default, px (tokens `--mixer-min-h`, §2.6). */
  const MIN_HEIGHT_PX = 176
  const MAX_HEIGHT_PX = 340
  const DEFAULT_HEIGHT_PX = 200
  const MAX_WINDOW_FRACTION = 0.55
  const KEY_RESIZE_STEP_PX = 8
  /** Strip width including its border, px (token `--strip-w`). */
  const STRIP_W_PX = 88
  /** Above this many tracks, only strips near the viewport are mounted. */
  const VIRTUALIZE_ABOVE = 24
  const OVERSCAN_STRIPS = 2

  let stripsEl: HTMLDivElement | undefined = $state()
  let scrollLeft = $state(0)
  let viewW = $state(0)

  let resizing = false
  let resizeStartY = 0
  let resizeStartHeight = 0

  const tracks = $derived(store.project?.tracks ?? [])
  const range = $derived(stripRange(tracks.length, scrollLeft, viewW))
  const visible = $derived(tracks.slice(range.first, range.last))
  const leadPx = $derived(range.first * STRIP_W_PX)
  const trailPx = $derived((tracks.length - range.last) * STRIP_W_PX)

  /** Index range of strips to mount: everything, or a window around the scroll position. */
  function stripRange(count: number, scroll: number, width: number): { first: number; last: number } {
    if (count <= VIRTUALIZE_ABOVE) return { first: 0, last: count }
    const first = Math.max(0, Math.floor(scroll / STRIP_W_PX) - OVERSCAN_STRIPS)
    const last = Math.min(count, Math.ceil((scroll + width) / STRIP_W_PX) + OVERSCAN_STRIPS)
    return { first, last }
  }

  function clampHeight(px: number): number {
    const max = Math.min(MAX_HEIGHT_PX, Math.floor(window.innerHeight * MAX_WINDOW_FRACTION))
    return Math.max(MIN_HEIGHT_PX, Math.min(max, Math.round(px)))
  }

  function onScroll(e: Event): void {
    scrollLeft = (e.currentTarget as HTMLElement).scrollLeft
  }

  function onGripDown(e: PointerEvent): void {
    if (e.button !== 0) return
    resizing = true
    resizeStartY = e.clientY
    resizeStartHeight = store.view.mixerHeight
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
    e.preventDefault()
  }

  function onGripMove(e: PointerEvent): void {
    if (!resizing) return
    // Dragging up (smaller clientY) makes the body taller.
    store.setMixerHeight(clampHeight(resizeStartHeight + (resizeStartY - e.clientY)))
  }

  function onGripUp(e: PointerEvent): void {
    if (!resizing) return
    resizing = false
    const grip = e.currentTarget as HTMLElement
    if (grip.hasPointerCapture(e.pointerId)) grip.releasePointerCapture(e.pointerId)
  }

  function onGripKey(e: KeyboardEvent): void {
    if (e.key !== 'ArrowUp' && e.key !== 'ArrowDown') return
    e.preventDefault()
    const delta = e.key === 'ArrowUp' ? KEY_RESIZE_STEP_PX : -KEY_RESIZE_STEP_PX
    store.setMixerHeight(clampHeight(store.view.mixerHeight + delta))
  }

  function onGripReset(): void {
    store.setMixerHeight(DEFAULT_HEIGHT_PX)
  }
</script>

<section class="mixer" role="group" aria-label="Mixer">
  <div class="bar">
    <div
      class="grip"
      role="slider"
      tabindex="0"
      aria-orientation="vertical"
      aria-label="Resize mixer"
      aria-valuemin={MIN_HEIGHT_PX}
      aria-valuemax={MAX_HEIGHT_PX}
      aria-valuenow={store.view.mixerHeight}
      title="Drag to resize; double-click resets"
      onpointerdown={onGripDown}
      onpointermove={onGripMove}
      onpointerup={onGripUp}
      onpointercancel={onGripUp}
      ondblclick={onGripReset}
      onkeydown={onGripKey}
    ></div>
    <span class="label-caps title">MIXER</span>
    <button
      type="button"
      class="btn btn-ghost btn-icon toggle"
      aria-expanded={store.view.mixerOpen}
      aria-label="Toggle mixer"
      title="Toggle mixer (Ctrl+M)"
      onclick={() => store.toggleMixer()}
    >
      <svg viewBox="0 0 24 24" aria-hidden="true"><path d="M6 9l6 6 6-6" /></svg>
    </button>
  </div>

  <div class="body" hidden={!store.view.mixerOpen} style:height="{store.view.mixerHeight}px">
    <div class="strips" bind:this={stripsEl} bind:clientWidth={viewW} onscroll={onScroll}>
      <div class="spacer" style:width="{leadPx}px" aria-hidden="true"></div>
      {#each visible as track (track.id)}
        <ChannelStrip trackId={track.id} />
      {/each}
      <div class="spacer" style:width="{trailPx}px" aria-hidden="true"></div>
    </div>
    <div class="master-pin">
      <MasterStrip />
    </div>
  </div>
</section>

<style>
  .mixer {
    position: relative;
    z-index: var(--z-mixer);
    display: flex;
    flex-direction: column;
  }

  .bar {
    position: relative;
    display: flex;
    flex: none;
    align-items: center;
    gap: var(--sp-4);
    height: var(--mixer-bar-h);
    padding: 0 var(--sp-4) 0 var(--sp-5);
    background: var(--bg-1);
    border-top: 1px solid var(--line);
    box-shadow: var(--shadow-1);
  }

  .grip {
    position: absolute;
    top: 0;
    right: 0;
    left: 0;
    height: 6px;
    cursor: ns-resize;
  }

  .title {
    flex: 1 1 auto;
  }

  .toggle svg {
    width: 16px;
    height: 16px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.75;
    stroke-linecap: round;
    stroke-linejoin: round;
    transition: transform var(--dur-base) var(--ease-out);
  }

  .toggle[aria-expanded='false'] svg {
    transform: rotate(180deg);
  }

  .body {
    display: flex;
    flex: none;
    min-height: 0;
    background: var(--bg-0);
    box-shadow: inset 0 1px 0 var(--line);
  }

  .strips {
    display: flex;
    flex: 1 1 auto;
    min-width: 0;
    overflow-x: auto;
    overflow-y: hidden;
  }

  .spacer {
    flex: none;
    height: 1px;
  }

  .master-pin {
    position: relative;
    flex: none;
    border-left: 1px solid var(--line-strong);
  }

  .master-pin::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -12px;
    width: 12px;
    background: linear-gradient(to right, transparent, var(--scrim));
    pointer-events: none;
  }
</style>
