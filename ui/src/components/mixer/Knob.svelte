<script lang="ts">
  import {
    clampPan,
    dragStep,
    keyStep,
    panAriaValue,
    panShortLabel,
    panSpokenLabel,
    panToAngle,
    wheelStep,
  } from '../../lib/knob'

  interface Props {
    /** Pan, −1 (left) … +1 (right). */
    value: number
    /** Accessible name, e.g. "Vocals pan". */
    label: string
    /** Live edit, called on every change. */
    onlive: (pan: number) => void
    /** Gesture end: pointer-up, keyup, blur, wheel idle (400 ms), or reset. */
    onend: () => void
  }
  let { value, label, onlive, onend }: Props = $props()

  const IDLE_END_MS = 400
  /** Keeps the dragged value on screen until the store's snapshot catches up. */
  const SETTLE_MS = 300

  /** Pan during and just after a gesture; the prop can lag behind live edits. */
  let livePan = $state<number | null>(null)
  /** Readout shown on hover, focus and drag. */
  let showReadout = $state(false)
  let knobEl: HTMLDivElement | undefined = $state()

  /** Gesture in progress; reactive because the template shows the active ring. */
  let active = $state(false)
  let dragging = false
  let lastY = 0
  let idleTimer: ReturnType<typeof setTimeout> | undefined
  let settleTimer: ReturnType<typeof setTimeout> | undefined

  const shown = $derived(livePan ?? clampPan(value))
  const angle = $derived(panToAngle(shown))
  const shortLabel = $derived(panShortLabel(shown))
  const ariaNow = $derived(panAriaValue(shown))
  const spokenLabel = $derived(panSpokenLabel(shown))

  $effect(() => {
    const el = knobEl
    if (!el) return
    // Non-passive so a wheel over the knob does not also scroll the strip row.
    const onWheel = (e: WheelEvent): void => {
      const current = livePan ?? clampPan(value)
      const next = wheelStep(current, e.deltaY, e.shiftKey)
      if (next === current) return
      e.preventDefault()
      if (!active) beginGesture()
      setPan(next)
      armIdle()
    }
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => {
      el.removeEventListener('wheel', onWheel)
      clearTimeout(idleTimer)
      clearTimeout(settleTimer)
    }
  })

  function beginGesture(): void {
    clearTimeout(settleTimer)
    active = true
  }

  function setPan(pan: number): void {
    const next = clampPan(pan)
    livePan = next
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
      livePan = null
    }, SETTLE_MS)
  }

  function armIdle(): void {
    clearTimeout(idleTimer)
    idleTimer = setTimeout(endGesture, IDLE_END_MS)
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0) return
    beginGesture()
    dragging = true
    showReadout = true
    lastY = e.clientY
    ;(e.currentTarget as HTMLElement).setPointerCapture(e.pointerId)
  }

  function onPointerMove(e: PointerEvent): void {
    if (!dragging) return
    const dy = e.clientY - lastY
    lastY = e.clientY
    setPan(dragStep(livePan ?? clampPan(value), dy, e.shiftKey))
  }

  function onPointerUp(e: PointerEvent): void {
    if (!dragging) return
    const el = e.currentTarget as HTMLElement
    if (el.hasPointerCapture(e.pointerId)) el.releasePointerCapture(e.pointerId)
    showReadout = false
    endGesture()
  }

  function onDoubleClick(): void {
    beginGesture()
    setPan(0)
    endGesture()
  }

  function onKeyDown(e: KeyboardEvent): void {
    const next = keyStep(livePan ?? clampPan(value), e.key, e.shiftKey)
    if (next === null) return
    e.preventDefault()
    if (!active) beginGesture()
    setPan(next)
    armIdle()
  }

  function onKeyUp(): void {
    if (active && !dragging) endGesture()
  }

  function onBlur(): void {
    showReadout = false
    if (active) endGesture()
  }
</script>

<div
  class="knob"
  class:is-active={active}
  bind:this={knobEl}
  role="slider"
  tabindex="0"
  aria-label={label}
  aria-valuemin={-100}
  aria-valuemax={100}
  aria-valuenow={ariaNow}
  aria-valuetext={spokenLabel}
  title={shortLabel}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={onPointerUp}
  onpointerenter={() => (showReadout = true)}
  onpointerleave={() => (showReadout = dragging)}
  ondblclick={onDoubleClick}
  onkeydown={onKeyDown}
  onkeyup={onKeyUp}
  onfocus={() => (showReadout = true)}
  onblur={onBlur}
>
  <span class="detent" aria-hidden="true"></span>
  <span class="pointer" style:--angle="{angle}deg" aria-hidden="true"></span>
  {#if showReadout}
    <span class="readout" aria-hidden="true">{shortLabel}</span>
  {/if}
</div>

<style>
  .knob {
    position: relative;
    flex: none;
    width: 36px;
    height: 36px;
    background: linear-gradient(var(--bg-4), var(--bg-2));
    border: 1px solid var(--line-strong);
    border-radius: 50%;
    box-shadow: var(--shadow-1);
    touch-action: none;
  }

  .knob:hover {
    border-color: var(--bg-5);
  }

  .knob.is-active {
    border-color: var(--accent-lo);
  }

  .pointer {
    position: absolute;
    inset: 0;
    transform: rotate(var(--angle));
    transition: transform var(--dur-fast) var(--ease-out);
    pointer-events: none;
  }

  .pointer::after {
    content: '';
    position: absolute;
    top: 4px;
    left: 50%;
    width: 2px;
    height: 12px;
    margin-left: -1px;
    background: var(--accent);
    border-radius: 1px;
  }

  .detent {
    position: absolute;
    top: -4px;
    left: 50%;
    width: 2px;
    height: 2px;
    margin-left: -1px;
    background: var(--text-muted);
    pointer-events: none;
  }

  .readout {
    position: absolute;
    top: calc(100% + var(--sp-2));
    left: 50%;
    z-index: var(--z-popover);
    padding: 0 var(--sp-3);
    transform: translateX(-50%);
    color: var(--text);
    background: var(--bg-1);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-xs);
    font: var(--fw-medium) var(--fs-xs) / 16px var(--font-label);
    white-space: nowrap;
    pointer-events: none;
  }
</style>
