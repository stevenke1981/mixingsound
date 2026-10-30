<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import { scrollExtent } from '../../lib/geometry'

  const MIN_THUMB_PX = 24
  const KEY_STEP_PX = 40

  interface DragState {
    startX: number
    startScroll: number
  }

  let trackEl: HTMLDivElement | undefined = $state()
  let trackW = $state(0)
  let drag: DragState | null = null

  const view = $derived({
    scroll: store.view.scrollFrames,
    fpp: store.view.fpp,
    viewportW: store.view.viewportW,
  })
  const extent = $derived(
    scrollExtent(store.projectEnd, store.sampleRate, view.scroll, view.fpp, view.viewportW),
  )
  const visibleFrames = $derived(view.viewportW * view.fpp)
  const totalFrames = $derived(Math.max(1, extent))
  const scrollMax = $derived(Math.max(1, totalFrames - visibleFrames))
  const thumbW = $derived(
    trackW <= 0 ? 0 : Math.max(MIN_THUMB_PX, Math.min(trackW, (trackW * visibleFrames) / totalFrames)),
  )
  const travel = $derived(Math.max(0, trackW - thumbW))
  const thumbX = $derived(travel * Math.min(1, Math.max(0, view.scroll / scrollMax)))

  function onThumbDown(e: PointerEvent): void {
    e.stopPropagation()
    if (!e.currentTarget || !(e.currentTarget instanceof HTMLElement)) return
    e.currentTarget.setPointerCapture(e.pointerId)
    drag = { startX: e.clientX, startScroll: view.scroll }
  }

  function onThumbMove(e: PointerEvent): void {
    const d = drag
    if (!d || travel <= 0) return
    const frames = ((e.clientX - d.startX) * scrollMax) / travel
    store.scrollToFrame(d.startScroll + frames)
  }

  function onThumbUp(): void {
    drag = null
  }

  /** Click on the track outside the thumb: centre the thumb under the pointer. */
  function onTrackDown(e: PointerEvent): void {
    if (e.button !== 0 || !trackEl || travel <= 0) return
    const x = e.clientX - trackEl.getBoundingClientRect().left
    const fraction = Math.min(1, Math.max(0, (x - thumbW / 2) / travel))
    store.scrollToFrame(fraction * scrollMax)
  }

  function onKey(e: KeyboardEvent): void {
    switch (e.key) {
      case 'ArrowLeft':
        e.preventDefault()
        store.scrollByPx(-KEY_STEP_PX)
        return
      case 'ArrowRight':
        e.preventDefault()
        store.scrollByPx(KEY_STEP_PX)
        return
      case 'Home':
        e.preventDefault()
        store.scrollToFrame(0)
        return
      case 'End':
        e.preventDefault()
        store.scrollToFrame(scrollMax)
        return
    }
  }
</script>

<div
  class="hscroll"
  bind:this={trackEl}
  bind:clientWidth={trackW}
  role="scrollbar"
  aria-orientation="horizontal"
  aria-label="Timeline scroll position"
  aria-controls="timeline-body"
  aria-valuemin={0}
  aria-valuemax={Math.round(scrollMax)}
  aria-valuenow={Math.round(view.scroll)}
  tabindex="0"
  onpointerdown={onTrackDown}
  onkeydown={onKey}
>
  <div
    class="thumb"
    style:width="{thumbW}px"
    style:transform="translateX({thumbX}px)"
    onpointerdown={onThumbDown}
    onpointermove={onThumbMove}
    onpointerup={onThumbUp}
    onpointercancel={onThumbUp}
    role="presentation"
  ></div>
</div>

<style>
  .hscroll {
    position: relative;
    width: 100%;
    height: 12px;
    background: var(--bg-0);
    touch-action: none;
  }

  .thumb {
    position: absolute;
    top: 2px;
    bottom: 2px;
    left: 0;
    background: var(--bg-5);
    border-radius: var(--r-pill);
  }

  .thumb:hover {
    background: var(--accent-lo);
  }
</style>
