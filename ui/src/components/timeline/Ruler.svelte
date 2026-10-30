<script lang="ts">
  import { store, type LoopRegion } from '../../lib/store.svelte'
  import { RULER_H, ZOOM_STEP, frameToX, xToFrame } from '../../lib/geometry'
  import { ticks } from '../../lib/ruler'
  import { SNAP_PX, snap, snapCandidates } from '../../lib/snap'
  import Playhead from './Playhead.svelte'
  import { type LanePalette, readLanePalette } from './drawClip'

  // Ruler layout (ui-spec §2.4): ticks above, a 10 px loop band at the bottom.
  const BAND_H = 10
  const TICK_ZONE_H = RULER_H - BAND_H
  const HEAD_ZONE_H = 12
  const HEAD_HIT_PX = 7
  const EDGE_HIT_PX = 6
  const DRAG_PX = 4
  const MIN_LOOP_SEC = 0.05
  const MAJOR_TICK_PX = 10
  const MINOR_TICK_PX = 5
  const LABEL_INSET_PX = 3
  const LABEL_TOP_PX = 3
  const HATCH_STEP_PX = 6
  const DISABLED_WASH_ALPHA = 0.3
  const LOOP_HANDLE_PX = 6

  type Mode = 'pending' | 'scrub' | 'loop' | 'edge-start' | 'edge-end'

  interface Point {
    x: number
    y: number
  }

  interface RulerDrag {
    mode: Mode
    startX: number
    /** Loop mode: drag anchor. Edge modes: the fixed opposite edge. Scrub/pending: press frame. */
    anchor: number
    candidates: number[]
  }

  const pal: LanePalette = readLanePalette(getComputedStyle(document.documentElement))

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let raf = 0
  let cssW = 0
  let cssH = 0
  let dpr = 1
  let drag: RulerDrag | null = null
  let preview: LoopRegion | null = null
  let scrubFrame = 0
  let scrubRaf = 0
  let cursor = ''

  function requestDraw(): void {
    if (raf !== 0) return
    raf = requestAnimationFrame(paint)
  }

  function resize(el: HTMLCanvasElement): void {
    dpr = window.devicePixelRatio || 1
    cssW = el.clientWidth
    cssH = el.clientHeight
    el.width = Math.max(1, Math.round(cssW * dpr))
    el.height = Math.max(1, Math.round(cssH * dpr))
    requestDraw()
  }

  function paint(): void {
    raf = 0
    const el = canvasEl
    const ctx = el?.getContext('2d')
    if (!el || !ctx || cssW === 0 || cssH === 0) return
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.fillStyle = pal.bg1
    ctx.fillRect(0, 0, cssW, cssH)
    ctx.fillStyle = pal.bg0
    ctx.fillRect(0, TICK_ZONE_H, cssW, BAND_H)
    ctx.fillStyle = pal.line
    ctx.fillRect(0, TICK_ZONE_H - 1, cssW, 1)
    paintTicks(ctx)
    const region = preview ?? store.loop
    if (region) paintLoop(ctx, region, preview !== null || store.loopEnabled)
  }

  function paintTicks(ctx: CanvasRenderingContext2D): void {
    const v = store.view
    ctx.font = `${pal.fs2xs}px ${pal.fontLabel}`
    ctx.textBaseline = 'top'
    for (const t of ticks(v.scrollFrames, v.fpp, cssW, store.sampleRate)) {
      const x = Math.round(frameToX(t.frame, v.scrollFrames, v.fpp))
      const len = t.major ? MAJOR_TICK_PX : MINOR_TICK_PX
      ctx.fillStyle = t.major ? pal.textMuted : pal.line
      ctx.fillRect(x, TICK_ZONE_H - len, 1, len)
      if (t.major && t.label) {
        ctx.fillStyle = pal.textMuted
        ctx.fillText(t.label, x + LABEL_INSET_PX, LABEL_TOP_PX)
      }
    }
  }

  /** Loop band: wash, 2 px top edge and 6 px end handles; disabled = desaturated and hatched. */
  function paintLoop(ctx: CanvasRenderingContext2D, region: LoopRegion, enabled: boolean): void {
    const v = store.view
    const x0 = frameToX(region.start, v.scrollFrames, v.fpp)
    const x1 = frameToX(region.end, v.scrollFrames, v.fpp)
    const width = Math.max(1, x1 - x0)
    const colour = enabled ? pal.accent : pal.textDisabled
    ctx.save()
    ctx.globalAlpha = enabled ? 1 : DISABLED_WASH_ALPHA
    ctx.fillStyle = enabled ? pal.accentWash : colour
    ctx.fillRect(x0, TICK_ZONE_H, width, BAND_H)
    ctx.globalAlpha = 1
    ctx.fillStyle = colour
    ctx.fillRect(x0, TICK_ZONE_H, width, 2)
    ctx.fillRect(x0, TICK_ZONE_H, LOOP_HANDLE_PX, BAND_H)
    ctx.fillRect(x0 + width - LOOP_HANDLE_PX, TICK_ZONE_H, LOOP_HANDLE_PX, BAND_H)
    if (!enabled) paintHatch(ctx, x0, width)
    ctx.restore()
  }

  function paintHatch(ctx: CanvasRenderingContext2D, x0: number, width: number): void {
    ctx.beginPath()
    ctx.rect(x0, TICK_ZONE_H, width, BAND_H)
    ctx.clip()
    ctx.strokeStyle = pal.textMuted
    ctx.lineWidth = 1
    ctx.beginPath()
    for (let k = 0; k < width + BAND_H; k += HATCH_STEP_PX) {
      ctx.moveTo(x0 + k, TICK_ZONE_H + BAND_H)
      ctx.lineTo(x0 + k + BAND_H, TICK_ZONE_H)
    }
    ctx.stroke()
  }

  // ---- Pointer --------------------------------------------------------------------------

  function localPoint(e: MouseEvent): Point {
    const rect = canvasEl?.getBoundingClientRect()
    return { x: e.clientX - (rect?.left ?? 0), y: e.clientY - (rect?.top ?? 0) }
  }

  function frameAt(x: number): number {
    return xToFrame(x, store.view.scrollFrames, store.view.fpp)
  }

  function minLoopFrames(): number {
    return store.sampleRate * MIN_LOOP_SEC
  }

  function snapped(frame: number, e: PointerEvent): number {
    const v = store.view
    const project = store.project
    if (!project || e.altKey) return frame
    const candidates = snapCandidates(project, null, store.position, store.loop)
    return snap(candidates, frame, SNAP_PX * v.fpp, true).frame
  }

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 || !canvasEl) return
    const p = localPoint(e)
    const frame = frameAt(p.x)
    const v = store.view
    const headX = frameToX(store.position, v.scrollFrames, v.fpp)
    if (p.y < HEAD_ZONE_H && Math.abs(p.x - headX) <= HEAD_HIT_PX) {
      drag = { mode: 'scrub', startX: p.x, anchor: frame, candidates: [] }
      scheduleSeek(frame)
    } else if (p.y >= TICK_ZONE_H) {
      drag = bandDrag(p, frame)
    } else {
      drag = { mode: 'pending', startX: p.x, anchor: frame, candidates: [] }
    }
    canvasEl.setPointerCapture(e.pointerId)
  }

  /** Band press: a loop edge handle when close to one, otherwise a new loop from here. */
  function bandDrag(p: Point, frame: number): RulerDrag {
    const loop = store.loop
    const v = store.view
    if (loop) {
      const startX = frameToX(loop.start, v.scrollFrames, v.fpp)
      const endX = frameToX(loop.end, v.scrollFrames, v.fpp)
      const toStart = Math.abs(p.x - startX)
      const toEnd = Math.abs(p.x - endX)
      if (Math.min(toStart, toEnd) <= EDGE_HIT_PX) {
        return toStart <= toEnd
          ? { mode: 'edge-start', startX: p.x, anchor: loop.end, candidates: [] }
          : { mode: 'edge-end', startX: p.x, anchor: loop.start, candidates: [] }
      }
    }
    return { mode: 'loop', startX: p.x, anchor: frame, candidates: [] }
  }

  function onPointerMove(e: PointerEvent): void {
    const p = localPoint(e)
    if (!drag) {
      updateCursor(p)
      return
    }
    const d = drag
    if (d.mode === 'scrub') {
      scheduleSeek(frameAt(p.x))
      return
    }
    const active: RulerDrag =
      d.mode === 'pending' && Math.abs(p.x - d.startX) >= DRAG_PX ? { ...d, mode: 'loop' } : d
    drag = active
    if (active.mode === 'pending') return
    const cur = snapped(frameAt(p.x), e)
    preview = previewRegion(active, cur)
    requestDraw()
  }

  function previewRegion(d: RulerDrag, cur: number): LoopRegion {
    const min = minLoopFrames()
    if (d.mode === 'edge-start') {
      return { start: Math.max(0, Math.min(cur, d.anchor - min)), end: d.anchor }
    }
    if (d.mode === 'edge-end') {
      return { start: d.anchor, end: Math.max(cur, d.anchor + min) }
    }
    return { start: Math.max(0, Math.min(d.anchor, cur)), end: Math.max(d.anchor, cur) }
  }

  function onPointerUp(): void {
    const d = drag
    drag = null
    if (!d) return
    if (d.mode === 'pending') {
      void store.seek(Math.round(d.anchor))
    } else if (d.mode !== 'scrub') {
      commitLoop()
    }
    preview = null
    requestDraw()
  }

  function commitLoop(): void {
    const region = preview
    if (!region || region.end - region.start < minLoopFrames()) return
    const current = store.loop
    if (current && current.start === region.start && current.end === region.end) return
    void store.setLoopRegion(Math.round(region.start), Math.round(region.end))
  }

  function scheduleSeek(frame: number): void {
    scrubFrame = frame
    if (scrubRaf !== 0) return
    scrubRaf = requestAnimationFrame(() => {
      scrubRaf = 0
      void store.seek(Math.round(scrubFrame))
    })
  }

  function updateCursor(p: Point): void {
    const next = cursorAt(p)
    if (next === cursor || !canvasEl) return
    cursor = next
    canvasEl.style.cursor = next
  }

  function cursorAt(p: Point): string {
    const v = store.view
    if (p.y < HEAD_ZONE_H && Math.abs(p.x - frameToX(store.position, v.scrollFrames, v.fpp)) <= HEAD_HIT_PX) {
      return 'ew-resize'
    }
    const loop = store.loop
    if (loop && p.y >= TICK_ZONE_H) {
      const nearEdge =
        Math.abs(p.x - frameToX(loop.start, v.scrollFrames, v.fpp)) <= EDGE_HIT_PX ||
        Math.abs(p.x - frameToX(loop.end, v.scrollFrames, v.fpp)) <= EDGE_HIT_PX
      if (nearEdge) return 'ew-resize'
    }
    return 'pointer'
  }

  function onDoubleClick(e: MouseEvent): void {
    if (localPoint(e).y >= TICK_ZONE_H) void store.clearLoop()
  }

  function onWheel(e: WheelEvent): void {
    e.preventDefault()
    if (e.ctrlKey) {
      const up = (e.deltaY || e.deltaX) < 0
      store.zoomBy(up ? ZOOM_STEP : 1 / ZOOM_STEP, localPoint(e).x)
      return
    }
    store.scrollByPx(e.deltaX !== 0 ? e.deltaX : e.deltaY)
  }

  // ---- Effects --------------------------------------------------------------------------

  $effect(() => {
    const el = canvasEl
    if (!el) return
    const observer = new ResizeObserver(() => resize(el))
    observer.observe(el)
    resize(el)
    return () => observer.disconnect()
  })

  $effect(() => {
    const el = canvasEl
    if (!el) return
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  })

  $effect(() => {
    void store.snapshot
    void store.view.fpp
    void store.view.scrollFrames
    void store.view.viewportW
    void store.loop
    void store.loopEnabled
    requestDraw()
  })

  $effect(() => {
    return () => {
      if (raf !== 0) cancelAnimationFrame(raf)
      if (scrubRaf !== 0) cancelAnimationFrame(scrubRaf)
      raf = 0
      scrubRaf = 0
    }
  })
</script>

<div class="ruler">
  <canvas
    bind:this={canvasEl}
    class="ruler-canvas"
    aria-hidden="true"
    onpointerdown={onPointerDown}
    onpointermove={onPointerMove}
    onpointerup={onPointerUp}
    onpointercancel={onPointerUp}
    ondblclick={onDoubleClick}
  ></canvas>
  <Playhead part="head" />
</div>

<style>
  .ruler {
    position: relative;
    width: 100%;
    height: 100%;
    overflow: hidden;
  }

  .ruler-canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    touch-action: none;
  }
</style>
