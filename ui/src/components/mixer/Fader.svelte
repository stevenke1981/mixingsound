<script lang="ts">
  import type { Snippet } from 'svelte'
  import { store } from '../../lib/store.svelte'
  import {
    clampGainDb,
    dbToFaderPos,
    faderKeyDb,
    faderPosToDb,
    faderScaleLayout,
    formatDbReadout,
    parseDbInput,
  } from '../../lib/dbScale'
  import { MAX_TRACK_GAIN_DB, MIN_GAIN_DB } from '../../lib/types'

  interface Props {
    /** Current gain in dB as the store reports it. */
    value: number
    /** Accessible name, e.g. "Vocals volume". */
    label: string
    /** Live edit; called on every move, throttled by the store. */
    onlive: (db: number) => void
    /** Gesture end: pointer-up, keyup, blur, or 400 ms idle. */
    onend: () => void
    /** Optional meter placed to the right of the slot. */
    meter?: Snippet
  }
  let { value, label, onlive, onend, meter }: Props = $props()

  /** Matches the cap height in Fader.svelte CSS (`.cap`). */
  const CAP_H = 16
  const IDLE_END_MS = 400
  /** Keeps the dragged value on screen until the store's snapshot catches up. */
  const SETTLE_MS = 300

  /** dB shown during and just after a gesture; the prop can lag behind live edits. */
  let gestureDb = $state<number | null>(null)
  /** Gain-field draft text while the user is typing; null shows the current value. */
  let draft = $state<string | null>(null)
  let trackH = $state(0)
  let trackEl: HTMLDivElement | undefined = $state()

  let active = false
  let dragging = false
  let gesturePos = 0
  let lastY = 0
  let idleTimer: ReturnType<typeof setTimeout> | undefined
  let settleTimer: ReturnType<typeof setTimeout> | undefined

  const shown = $derived(gestureDb ?? clampGainDb(value))
  const pos = $derived(dbToFaderPos(shown))
  const ticks = $derived(faderScaleLayout(trackH, CAP_H))
  const ariaNow = $derived(Math.round(shown * 10) / 10)
  const valueText = $derived(`${formatDbReadout(shown)} dB`)

  $effect(() => () => {
    clearTimeout(idleTimer)
    clearTimeout(settleTimer)
  })

  function beginGesture(): void {
    clearTimeout(settleTimer)
    active = true
  }

  function setGain(db: number): void {
    const next = clampGainDb(db)
    gestureDb = next
    onlive(next)
  }

  function endGesture(): void {
    if (!active) return
    active = false
    dragging = false
    clearTimeout(idleTimer)
    onend()
    clearTimeout(settleTimer)
    settleTimer = setTimeout(() => {
      gestureDb = null
    }, SETTLE_MS)
  }

  function armIdle(): void {
    clearTimeout(idleTimer)
    idleTimer = setTimeout(endGesture, IDLE_END_MS)
  }

  /** Fader position under a pointer: the cap centre follows the pointer's y. */
  function positionFromPointer(clientY: number): number {
    const rect = trackEl?.getBoundingClientRect()
    if (!rect) return pos
    const travel = Math.max(1, rect.height - CAP_H)
    const clamped = (rect.bottom - CAP_H / 2 - clientY) / travel
    return Math.min(1, Math.max(0, clamped))
  }

  function travelPx(): number {
    return Math.max(1, (trackEl?.clientHeight ?? 0) - CAP_H)
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return
    const onCap = e.target instanceof Element && e.target.closest('.cap') !== null
    beginGesture()
    dragging = true
    lastY = e.clientY
    const slider = e.currentTarget as HTMLElement
    slider.setPointerCapture(e.pointerId)
    if (onCap) {
      gesturePos = pos
      return
    }
    gesturePos = positionFromPointer(e.clientY)
    setGain(faderPosToDb(gesturePos))
  }

  function onPointerMove(e: PointerEvent): void {
    if (!dragging) return
    const dy = e.clientY - lastY
    lastY = e.clientY
    const sensitivity = e.shiftKey ? 0.1 : 1
    gesturePos = Math.min(1, Math.max(0, gesturePos - (dy / travelPx()) * sensitivity))
    setGain(faderPosToDb(gesturePos))
  }

  function onPointerUp(e: PointerEvent): void {
    if (!dragging) return
    const slider = e.currentTarget as HTMLElement
    if (slider.hasPointerCapture(e.pointerId)) slider.releasePointerCapture(e.pointerId)
    endGesture()
  }

  function onDoubleClick(): void {
    beginGesture()
    setGain(0)
    endGesture()
  }

  function onKeyDown(e: KeyboardEvent): void {
    const next = faderKeyDb(shown, e.key, e.shiftKey)
    if (next === null) return
    e.preventDefault()
    if (!active) beginGesture()
    setGain(next)
    armIdle()
  }

  function onKeyUp(): void {
    if (active && !dragging) endGesture()
  }

  function onSliderBlur(): void {
    if (active) endGesture()
  }

  function onGainInput(e: Event): void {
    draft = (e.currentTarget as HTMLInputElement).value
  }

  function onGainKeyDown(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault()
      commitDraft()
    } else if (e.key === 'Escape') {
      e.stopPropagation()
      draft = null
      ;(e.currentTarget as HTMLInputElement).blur()
    }
  }

  function commitDraft(): void {
    const text = draft ?? ''
    draft = null
    const parsed = parseDbInput(text)
    if (parsed === null) {
      store.toast('warn', `"${text}" is not a gain in dB; enter −96 to +12`)
      return
    }
    beginGesture()
    setGain(parsed)
    endGesture()
  }

  function onGainBlur(): void {
    draft = null
  }

  function selectAll(e: FocusEvent): void {
    ;(e.currentTarget as HTMLInputElement).select()
  }
</script>

<div class="fader-block">
  <div class="fader-line">
    <div
      class="fader"
      role="slider"
      tabindex="0"
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemin={MIN_GAIN_DB}
      aria-valuemax={MAX_TRACK_GAIN_DB}
      aria-valuenow={ariaNow}
      aria-valuetext={valueText}
      onpointerdown={onPointerDown}
      onpointermove={onPointerMove}
      onpointerup={onPointerUp}
      onpointercancel={onPointerUp}
      ondblclick={onDoubleClick}
      onkeydown={onKeyDown}
      onkeyup={onKeyUp}
      onblur={onSliderBlur}
    >
      <div class="scale" aria-hidden="true">
        {#each ticks as tick (tick.db)}
          <span class="tick-label" class:is-zero={tick.isZero} style:top="{tick.y}px">{tick.text}</span>
        {/each}
      </div>
      <div class="track" bind:this={trackEl} bind:clientHeight={trackH}>
        <div class="slot"></div>
        <div class="cap" style:--pos={pos}></div>
      </div>
    </div>
    {#if meter}
      {@render meter()}
    {/if}
  </div>
  <input
    class="field num gain"
    type="text"
    inputmode="decimal"
    autocomplete="off"
    spellcheck="false"
    aria-label="{label} in dB"
    title="Gain in dB; Enter to apply"
    value={draft ?? formatDbReadout(shown)}
    oninput={onGainInput}
    onkeydown={onGainKeyDown}
    onblur={onGainBlur}
    onfocus={selectAll}
  />
</div>

<style>
  .fader-block {
    display: flex;
    flex: 1 1 auto;
    flex-direction: column;
    gap: var(--sp-3);
    min-height: 0;
  }

  .fader-line {
    display: flex;
    flex: 1 1 auto;
    min-height: 0;
  }

  .fader {
    position: relative;
    display: flex;
    align-self: stretch;
    border-radius: var(--r-xs);
    touch-action: none;
  }

  .fader:hover .cap {
    border-color: var(--bg-5);
  }

  .scale {
    position: relative;
    flex: none;
    width: 18px;
  }

  .tick-label {
    position: absolute;
    right: var(--sp-3);
    transform: translateY(-50%);
    color: var(--text-muted);
    font: var(--fw-medium) var(--fs-2xs) / 1 var(--font-label);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }

  .tick-label::after {
    content: '';
    position: absolute;
    top: 50%;
    right: calc(var(--sp-3) * -1 - 4px);
    width: 4px;
    height: 1px;
    background: var(--text-muted);
  }

  .tick-label.is-zero {
    color: var(--text-2);
  }

  .tick-label.is-zero::after {
    right: calc(var(--sp-3) * -1 - 8px);
    width: 8px;
    background: var(--text-2);
  }

  .track {
    position: relative;
    flex: none;
    align-self: stretch;
    width: 28px;
  }

  .slot {
    position: absolute;
    top: 0;
    bottom: 0;
    left: 50%;
    width: 4px;
    transform: translateX(-50%);
    background: var(--well);
    border-radius: var(--r-xs);
    box-shadow: var(--shadow-inset);
  }

  .cap {
    position: absolute;
    right: 0;
    left: 0;
    height: 16px;
    bottom: calc((100% - 16px) * var(--pos));
    background: linear-gradient(var(--bg-5), var(--bg-3));
    border: 1px solid var(--line-strong);
    border-radius: var(--r-xs);
    box-shadow: var(--shadow-1);
  }

  .cap::after {
    content: '';
    position: absolute;
    top: 50%;
    right: 3px;
    left: 3px;
    height: 2px;
    margin-top: -1px;
    background: var(--accent);
  }

  .gain {
    flex: none;
    width: 100%;
    height: 22px;
    padding: 0 var(--sp-3);
  }
</style>
