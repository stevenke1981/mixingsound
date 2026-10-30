<script lang="ts">
  import { untrack } from 'svelte'
  import { api } from '../../lib/api'
  import {
    computeFades,
    computeGain,
    computeMove,
    computeSplit,
    computeTrimLeft,
    computeTrimRight,
  } from '../../lib/clipEdit'
  import { LANE_H, ZOOM_STEP, frameToX, xToFrame } from '../../lib/geometry'
  import { CLIP_INSET_Y, HEADER_STRIP_H, hitTest, laneAt, type Hit } from '../../lib/hitTest'
  import { snap, snapCandidates, snapMove, SNAP_PX } from '../../lib/snap'
  import { store } from '../../lib/store.svelte'
  import type { Clip, Track } from '../../lib/types'
  import { WaveformCache } from '../../lib/waveformCache'
  import {
    type ClipGeo,
    type ClipGestureKind,
    type ClipLook,
    type GhostState,
    type LanePalette,
    type WaveSample,
    drawBubble,
    drawClip,
    drawLaneBackground,
    drawLoopTint,
    drawSnapGuide,
    formatClock,
    gestureBubble,
    readLanePalette,
  } from './drawClip'

  interface Props {
    /** The vertical scroll container shared with the track headers. */
    scroller?: HTMLElement
  }
  let { scroller }: Props = $props()

  interface Point {
    x: number
    /** Content y: 0 = top of the first lane. */
    y: number
  }

  interface Gesture {
    kind: ClipGestureKind
    clip: Clip
    origTrackIndex: number
    startX: number
    startY: number
    startDb: number
    /** Distance from the pointer to the grabbed fade edge, in frames (fades only). */
    grab: number
    candidates: number[]
    ghost: GhostState
    snapTarget: number | null
  }

  interface KeyEdit {
    clipId: number
    start: number
    trackIndex: number
  }

  interface Frame {
    ctx: CanvasRenderingContext2D
    scrollTop: number
    scroll: number
    fpp: number
    sr: number
    w: number
    h: number
  }

  const HINT_ID = 'timeline-keys'
  const KEYS_HINT =
    'Left and Right arrows nudge the selected clip 10 ms (Shift 100 ms, Alt one frame). ' +
    '[ and ] select the previous or next clip on the track. Alt with Up or Down moves the clip to another track. ' +
    'Escape cancels a drag.'
  const NUDGE_MS = 10
  const NUDGE_SHIFT_MS = 100
  const BUBBLE_OFFSET_PX = 6
  const CURSOR_BY_KIND: Record<ClipGestureKind, string> = {
    move: 'grabbing',
    trimL: 'ew-resize',
    trimR: 'ew-resize',
    fadeIn: 'col-resize',
    fadeOut: 'col-resize',
    gain: 'ns-resize',
  }

  const pal: LanePalette = readLanePalette(getComputedStyle(document.documentElement))
  const cache = new WaveformCache(api.getWaveform, requestDraw)

  let canvasEl: HTMLCanvasElement | undefined = $state()
  let raf = 0
  let cssW = 0
  let cssH = 0
  let dpr = 1
  let hoverClipId: number | null = null
  let gesture: Gesture | null = null
  let committing = false
  let keyState: KeyEdit | null = null
  let keyActive = false
  let lastSelectedId: number | null = null
  let lastCursor = ''

  // ---- Drawing schedule: one rAF, only when dirty ------------------------------------------

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
    const v = store.view
    const fr: Frame = {
      ctx,
      scrollTop: scroller?.scrollTop ?? 0,
      scroll: v.scrollFrames,
      fpp: v.fpp,
      sr: store.sampleRate,
      w: cssW,
      h: cssH,
    }
    const tracks = store.project?.tracks ?? []
    ctx.fillStyle = pal.bg0
    ctx.fillRect(0, 0, cssW, cssH)
    const [first, last] = visibleLanes(fr, tracks.length)
    for (let i = first; i <= last; i++) {
      drawLaneBackground(ctx, pal, i * LANE_H - fr.scrollTop, i, cssW)
    }
    paintLoop(fr)
    for (let i = first; i <= last; i++) {
      const track = tracks[i]
      if (track) paintClips(fr, track, i)
    }
    paintGhost(fr, tracks)
    const target = gesture?.snapTarget
    if (target !== null && target !== undefined) {
      drawSnapGuide(ctx, pal, frameToX(target, fr.scroll, fr.fpp), cssH)
    }
  }

  function visibleLanes(fr: Frame, count: number): [number, number] {
    const first = Math.max(0, Math.floor(fr.scrollTop / LANE_H))
    const last = Math.min(count - 1, Math.floor((fr.scrollTop + fr.h) / LANE_H))
    return [first, last]
  }

  function paintLoop(fr: Frame): void {
    const loop = store.loop
    if (!loop) return
    const x0 = frameToX(loop.start, fr.scroll, fr.fpp)
    const x1 = frameToX(loop.end, fr.scroll, fr.fpp)
    drawLoopTint(fr.ctx, pal, x0, x1, fr.h, store.loopEnabled)
  }

  function paintClips(fr: Frame, track: Track, index: number): void {
    const visStart = fr.scroll
    const visEnd = fr.scroll + fr.w * fr.fpp
    const base = trackAlpha(track)
    for (const clip of track.clips) {
      if (clip.start + clip.length <= visStart || clip.start >= visEnd) continue
      const dragging = gesture?.clip.id === clip.id
      const look = clipLook(track, clip, dragging ? 0.35 : base, false)
      const geo = clipGeo(fr, clip.start, clip.length, clip.fadeIn, clip.fadeOut, index)
      const sample = waveSampler(fr, clip.sourceId, clip, fr.w)
      drawClip(fr.ctx, pal, geo, look, sample)
    }
  }

  function trackAlpha(track: Track): number {
    if (track.mute) return 0.4
    if (store.anySolo && !track.solo) return 0.55
    return 1
  }

  function clipLook(track: Track, clip: Clip, alpha: number, ghost: boolean): ClipLook {
    const selected = store.selection.clipId === clip.id
    const hovered = hoverClipId === clip.id
    const handleOwner = hoverClipId ?? store.selection.clipId
    return {
      name: sourceName(clip.sourceId),
      color: track.color,
      gainDb: clip.gainDb,
      missing: store.isMissing(clip.sourceId),
      selected: selected || ghost,
      hovered,
      showHandles: handleOwner === clip.id && !ghost,
      ghost,
      alpha,
    }
  }

  function sourceName(sourceId: number): string {
    return store.sourceById(sourceId)?.name ?? 'Clip'
  }

  function clipGeo(
    fr: Frame,
    start: number,
    length: number,
    fadeIn: number,
    fadeOut: number,
    laneIndex: number,
  ): ClipGeo {
    return {
      x: frameToX(start, fr.scroll, fr.fpp),
      y: laneIndex * LANE_H - fr.scrollTop + CLIP_INSET_Y,
      w: length / fr.fpp,
      h: LANE_H - CLIP_INSET_Y * 2,
      viewW: fr.w,
      fadeInPx: fadeIn / fr.fpp,
      fadeOutPx: fadeOut / fr.fpp,
    }
  }

  /** Maps each canvas column to a min/max over the covering source frames (ui-spec §5.3). */
  function waveSampler(
    fr: Frame,
    sourceId: number,
    geom: { start: number; offset: number; length: number },
    viewW: number,
  ): WaveSample {
    const sourceLo = geom.offset
    const sourceHi = geom.offset + geom.length
    const viewCentre = xToFrame(viewW / 2, fr.scroll, fr.fpp) - geom.start
    const centre = geom.offset + Math.min(geom.length, Math.max(0, viewCentre))
    return (col: number): [number, number] | null => {
      const f0 = xToFrame(col, fr.scroll, fr.fpp) - geom.start + geom.offset
      const f1 = xToFrame(col + 1, fr.scroll, fr.fpp) - geom.start + geom.offset
      const from = Math.max(sourceLo, f0)
      const to = Math.min(sourceHi, f1)
      if (to <= from) return null
      const first = Math.floor(from)
      return cache.range(sourceId, first, Math.max(first + 1, Math.ceil(to)), fr.fpp, centre)
    }
  }

  function paintGhost(fr: Frame, tracks: Track[]): void {
    const g = gesture
    if (!g) return
    const origin = tracks[g.origTrackIndex]
    if (!origin) return
    const gh = g.ghost
    const geo = clipGeo(fr, gh.start, gh.length, gh.fadeIn, gh.fadeOut, gh.trackIndex)
    const look = clipLook(origin, g.clip, 1, true)
    drawClip(fr.ctx, pal, geo, look, waveSampler(fr, g.clip.sourceId, gh, fr.w))
    const text = gestureBubble(g.kind, gh, g.clip.start, fr.sr)
    drawBubble(fr.ctx, pal, geo.x + BUBBLE_OFFSET_PX, geo.y + HEADER_STRIP_H + BUBBLE_OFFSET_PX, text)
  }

  // ---- Coordinates, hit testing, cursor ----------------------------------------------------

  function pointOf(e: MouseEvent): Point {
    const rect = canvasEl?.getBoundingClientRect()
    return {
      x: e.clientX - (rect?.left ?? 0),
      y: e.clientY - (rect?.top ?? 0) + (scroller?.scrollTop ?? 0),
    }
  }

  function frameAt(x: number): number {
    return xToFrame(x, store.view.scrollFrames, store.view.fpp)
  }

  function hitAt(p: Point): Hit | null {
    const project = store.project
    if (!project) return null
    const v = store.view
    return hitTest(project, p.x, p.y, v.scrollFrames, v.fpp, hoverClipId ?? store.selection.clipId)
  }

  function cursorFor(hit: Hit | null, shift: boolean, ctrl: boolean): string {
    if (!hit) return 'default'
    switch (hit.kind) {
      case 'edgeL':
      case 'edgeR':
        return 'ew-resize'
      case 'fadeIn':
      case 'fadeOut':
        return 'col-resize'
      case 'body':
        if (shift) return 'crosshair'
        return ctrl ? 'ns-resize' : 'grab'
    }
  }

  function setCursor(cursor: string): void {
    if (!canvasEl || cursor === lastCursor) return
    lastCursor = cursor
    canvasEl.style.cursor = cursor
  }

  function updateHover(p: Point, e: PointerEvent): void {
    const hit = hitAt(p)
    const next = hit?.clipId ?? null
    if (next !== hoverClipId) {
      hoverClipId = next
      requestDraw()
    }
    setCursor(cursorFor(hit, e.shiftKey, e.ctrlKey))
  }

  // ---- Pointer gestures (ui-spec §3.1) -----------------------------------------------------

  function onPointerDown(e: PointerEvent): void {
    if (e.button !== 0 || committing || !canvasEl) return
    const project = store.project
    if (!project) return
    const p = pointOf(e)
    const hit = hitAt(p)
    if (!hit) {
      selectEmpty(p, laneAt(p.y, project.tracks.length))
      return
    }
    const info = store.clipById(hit.clipId)
    const trackIndex = project.tracks.findIndex((t) => t.id === hit.trackId)
    if (e.shiftKey && hit.kind === 'body') {
      splitAt(hit.clipId, p.x)
      return
    }
    if (!info || trackIndex < 0) return
    store.selectClip(info.clip.id)
    beginGesture(gestureKindOf(hit, e.ctrlKey), info.clip, trackIndex, p, e)
  }

  function gestureKindOf(hit: Hit, ctrl: boolean): ClipGestureKind {
    switch (hit.kind) {
      case 'edgeL':
        return 'trimL'
      case 'edgeR':
        return 'trimR'
      case 'fadeIn':
        return 'fadeIn'
      case 'fadeOut':
        return 'fadeOut'
      case 'body':
        return ctrl ? 'gain' : 'move'
    }
  }

  function selectEmpty(p: Point, lane: number | null): void {
    store.selectClip(null)
    const track = lane === null ? undefined : store.project?.tracks[lane]
    if (!track) return
    store.selectTrack(track.id)
    void store.seek(Math.round(frameAt(p.x)))
  }

  function splitAt(clipId: number, x: number): void {
    const info = store.clipById(clipId)
    if (!info) return
    const v = store.view
    const playheadX = frameToX(store.position, v.scrollFrames, v.fpp)
    const nearPlayhead = Math.abs(playheadX - x) <= SNAP_PX
    const at = Math.round(nearPlayhead ? store.position : frameAt(x))
    if (computeSplit(info.clip, at) === null) return
    void store.splitClip(clipId, at)
  }

  function grabOffset(kind: ClipGestureKind, clip: Clip, frame: number): number {
    if (kind === 'fadeIn') return clip.fadeIn - (frame - clip.start)
    if (kind === 'fadeOut') return clip.fadeOut - (clip.start + clip.length - frame)
    return 0
  }

  function beginGesture(kind: ClipGestureKind, clip: Clip, trackIndex: number, p: Point, e: PointerEvent): void {
    const project = store.project
    if (!canvasEl || !project) return
    gesture = {
      kind,
      clip,
      origTrackIndex: trackIndex,
      startX: p.x,
      startY: p.y,
      startDb: clip.gainDb,
      grab: grabOffset(kind, clip, frameAt(p.x)),
      candidates: snapCandidates(project, clip.id, store.position, store.loop),
      ghost: {
        start: clip.start,
        offset: clip.offset,
        length: clip.length,
        fadeIn: clip.fadeIn,
        fadeOut: clip.fadeOut,
        gainDb: clip.gainDb,
        trackIndex,
      },
      snapTarget: null,
    }
    canvasEl.setPointerCapture(e.pointerId)
    setCursor(CURSOR_BY_KIND[kind])
    window.addEventListener('keydown', onGestureKey, true)
    requestDraw()
  }

  function onPointerMove(e: PointerEvent): void {
    const p = pointOf(e)
    if (gesture) {
      if (!committing) updateGesture(gesture, p, e)
      return
    }
    updateHover(p, e)
  }

  function updateGesture(g: Gesture, p: Point, e: PointerEvent): void {
    const v = store.view
    const snapOn = !e.altKey
    const thr = SNAP_PX * v.fpp
    const dFrames = (p.x - g.startX) * v.fpp
    g.snapTarget = null
    switch (g.kind) {
      case 'move':
        previewMove(g, p, dFrames, snapOn, thr)
        break
      case 'trimL':
        previewTrimLeft(g, dFrames, snapOn, thr)
        break
      case 'trimR':
        previewTrimRight(g, dFrames, snapOn, thr)
        break
      case 'fadeIn':
      case 'fadeOut':
        previewFade(g, p, g.kind === 'fadeIn')
        break
      case 'gain':
        g.ghost = { ...g.ghost, gainDb: computeGain(g.startDb, p.y - g.startY, e.shiftKey) }
        break
    }
    requestDraw()
  }

  function previewMove(g: Gesture, p: Point, dFrames: number, snapOn: boolean, thr: number): void {
    const raw = Math.round(g.clip.start + dFrames)
    const snapped = snapMove(g.candidates, raw, g.clip.length, thr, snapOn)
    const start = computeMove(g.clip, snapped.frame - g.clip.start) ?? g.clip.start
    const lane = laneAt(p.y, store.project?.tracks.length ?? 0)
    g.ghost = { ...g.ghost, start, trackIndex: lane ?? g.ghost.trackIndex }
    g.snapTarget = snapped.target
  }

  function previewTrimLeft(g: Gesture, dFrames: number, snapOn: boolean, thr: number): void {
    const raw = Math.round(g.clip.start + dFrames)
    const snapped = snap(g.candidates, raw, thr, snapOn)
    const geom = computeTrimLeft(g.clip, snapped.frame) ?? g.clip
    g.ghost = { ...g.ghost, start: geom.start, offset: geom.offset, length: geom.length }
    g.snapTarget = snapped.target
  }

  function previewTrimRight(g: Gesture, dFrames: number, snapOn: boolean, thr: number): void {
    const raw = Math.round(g.clip.start + g.clip.length + dFrames)
    const snapped = snap(g.candidates, raw, thr, snapOn)
    const geom = computeTrimRight(g.clip, snapped.frame, sourceFrames(g.clip)) ?? g.clip
    g.ghost = { ...g.ghost, start: geom.start, offset: geom.offset, length: geom.length }
    g.snapTarget = snapped.target
  }

  function sourceFrames(clip: Clip): number {
    return store.sourceById(clip.sourceId)?.frames ?? clip.offset + clip.length
  }

  function previewFade(g: Gesture, p: Point, isFadeIn: boolean): void {
    const frame = frameAt(p.x)
    const clip = g.clip
    const raw = isFadeIn
      ? frame - clip.start + g.grab
      : clip.start + clip.length - frame + g.grab
    const fades = computeFades(clip, isFadeIn ? 'in' : 'out', Math.max(0, Math.round(raw)))
    const next = fades ?? clip
    g.ghost = { ...g.ghost, fadeIn: next.fadeIn, fadeOut: next.fadeOut }
  }

  async function onPointerUp(e: PointerEvent): Promise<void> {
    const g = gesture
    if (!g || committing) return
    updateGesture(g, pointOf(e), e)
    committing = true
    try {
      const commit = editFor(g)
      if (commit) {
        await commit()
        await store.endGesture()
      }
    } finally {
      finishGesture()
    }
  }

  /** The single store call for a finished gesture, or null when nothing changed. */
  function editFor(g: Gesture): (() => Promise<void>) | null {
    const { clip, ghost } = g
    switch (g.kind) {
      case 'move': {
        const track = store.project?.tracks[ghost.trackIndex]
        if (!track) return null
        if (ghost.start === clip.start && ghost.trackIndex === g.origTrackIndex) return null
        return () => store.moveClip(clip.id, track.id, ghost.start)
      }
      case 'trimL':
      case 'trimR':
        if (ghost.start === clip.start && ghost.offset === clip.offset && ghost.length === clip.length) {
          return null
        }
        return () => store.trimClip(clip.id, ghost.start, ghost.offset, ghost.length)
      case 'fadeIn':
      case 'fadeOut':
        if (ghost.fadeIn === clip.fadeIn && ghost.fadeOut === clip.fadeOut) return null
        return () => store.setClipFades(clip.id, ghost.fadeIn, ghost.fadeOut)
      case 'gain':
        if (ghost.gainDb === clip.gainDb) return null
        return () => store.setClipGain(clip.id, ghost.gainDb)
    }
  }

  function finishGesture(): void {
    gesture = null
    committing = false
    window.removeEventListener('keydown', onGestureKey, true)
    setCursor('default')
    requestDraw()
  }

  function cancelGesture(): void {
    if (!gesture || committing) return
    finishGesture()
  }

  /** Captured at window level so Escape cancels the drag before any other handler sees it. */
  function onGestureKey(e: KeyboardEvent): void {
    if (e.key !== 'Escape') return
    e.preventDefault()
    e.stopPropagation()
    cancelGesture()
  }

  function onPointerLeave(): void {
    if (gesture) return
    hoverClipId = null
    setCursor('default')
    requestDraw()
  }

  function handleWheel(e: WheelEvent): void {
    if (e.ctrlKey) {
      e.preventDefault()
      const up = (e.deltaY || e.deltaX) < 0
      store.zoomBy(up ? ZOOM_STEP : 1 / ZOOM_STEP, pointOf(e).x)
      return
    }
    if (e.shiftKey || e.deltaX !== 0) {
      e.preventDefault()
      store.scrollByPx(e.deltaX !== 0 ? e.deltaX : e.deltaY)
    }
  }

  // ---- Keyboard on the canvas --------------------------------------------------------------

  function selectedInfo(): { clip: Clip; track: Track } | undefined {
    const id = store.selection.clipId
    return id === null ? undefined : store.clipById(id)
  }

  function keyBase(clip: Clip, trackIndex: number): KeyEdit {
    const s = keyState
    return s && s.clipId === clip.id ? s : { clipId: clip.id, start: clip.start, trackIndex }
  }

  function sendKeyMove(clip: Clip, trackIndex: number, start: number): void {
    const track = store.project?.tracks[trackIndex]
    if (!track) return
    keyState = { clipId: clip.id, start, trackIndex }
    keyActive = true
    void store.moveClip(clip.id, track.id, start)
  }

  function nudge(deltaFrames: number): void {
    const info = selectedInfo()
    if (!info) return
    const trackIndex = store.project?.tracks.findIndex((t) => t.id === info.track.id) ?? -1
    const base = keyBase(info.clip, trackIndex)
    const start = computeMove(info.clip, base.start + deltaFrames - info.clip.start) ?? base.start
    sendKeyMove(info.clip, base.trackIndex, start)
  }

  function moveToTrack(dir: number): void {
    const info = selectedInfo()
    const tracks = store.project?.tracks
    if (!info || !tracks) return
    const base = keyBase(info.clip, tracks.findIndex((t) => t.id === info.track.id))
    const target = base.trackIndex + dir
    if (target < 0 || target >= tracks.length) return
    sendKeyMove(info.clip, target, base.start)
  }

  function revealFrame(frame: number): void {
    const v = store.view
    if (frame < v.scrollFrames || frame > v.scrollFrames + v.viewportW * v.fpp) {
      store.scrollToFrame(frame)
    }
  }

  function cycleClip(dir: number): void {
    const project = store.project
    if (!project) return
    const info = selectedInfo()
    const trackId = info?.track.id ?? store.selection.trackId
    const track = project.tracks.find((t) => t.id === trackId)
    if (!track || track.clips.length === 0) return
    const clips = [...track.clips].sort((a, b) => a.start - b.start)
    const idx = info ? clips.findIndex((c) => c.id === info.clip.id) : -1
    const next = idx < 0 ? (dir > 0 ? clips[0] : clips[clips.length - 1]) : clips[idx + dir]
    if (!next) return
    store.selectClip(next.id)
    revealFrame(next.start)
  }

  function onKeyDown(e: KeyboardEvent): void {
    if (gesture || committing) return
    switch (e.key) {
      case 'ArrowLeft':
      case 'ArrowRight': {
        if (!selectedInfo()) return
        e.preventDefault()
        const dir = e.key === 'ArrowRight' ? 1 : -1
        nudge(dir * nudgeFrames(e))
        return
      }
      case 'ArrowUp':
      case 'ArrowDown':
        if (!e.altKey) return
        e.preventDefault()
        moveToTrack(e.key === 'ArrowDown' ? 1 : -1)
        return
      case '[':
      case ']':
        e.preventDefault()
        cycleClip(e.key === ']' ? 1 : -1)
        return
    }
  }

  function nudgeFrames(e: KeyboardEvent): number {
    if (e.altKey) return 1
    const ms = e.shiftKey ? NUDGE_SHIFT_MS : NUDGE_MS
    return Math.round((store.sampleRate * ms) / 1000)
  }

  /** Keyboard edits send one moveClip per press; the key release closes the undo step. */
  function onKeyUp(): void {
    if (!keyActive) return
    keyActive = false
    keyState = null
    void store.endGesture()
  }

  // ---- Announcements -----------------------------------------------------------------------

  function describeClip(clipId: number): string {
    const info = store.clipById(clipId)
    if (!info) return ''
    const sr = store.sampleRate
    const index = store.project?.tracks.findIndex((t) => t.id === info.track.id) ?? -1
    return (
      `Clip ${sourceName(info.clip.sourceId)} selected on track ${index + 1}, ` +
      `starts ${formatClock(info.clip.start, sr)}, length ${formatClock(info.clip.length, sr)}`
    )
  }

  // ---- Effects -----------------------------------------------------------------------------

  $effect(() => {
    const el = canvasEl
    if (!el) return
    const observer = new ResizeObserver(() => resize(el))
    observer.observe(el)
    resize(el)
    return () => observer.disconnect()
  })

  $effect(() => {
    const el = scroller
    if (!el) return
    el.addEventListener('scroll', requestDraw, { passive: true })
    return () => el.removeEventListener('scroll', requestDraw)
  })

  $effect(() => {
    const el = canvasEl
    if (!el) return
    const onWheel = (e: WheelEvent): void => handleWheel(e)
    el.addEventListener('wheel', onWheel, { passive: false })
    return () => el.removeEventListener('wheel', onWheel)
  })

  $effect(() => {
    cache.reset(store.projectEpoch)
    requestDraw()
  })

  $effect(() => {
    void store.snapshot
    void store.view.fpp
    void store.view.scrollFrames
    void store.view.viewportW
    void store.view.viewportH
    void store.selection.clipId
    void store.selection.trackId
    void store.loop
    void store.loopEnabled
    requestDraw()
  })

  $effect(() => {
    const id = store.selection.clipId
    if (id === lastSelectedId) return
    lastSelectedId = id
    if (id === null) return
    untrack(() => store.announce(describeClip(id)))
  })

  $effect(() => {
    return () => {
      if (raf !== 0) cancelAnimationFrame(raf)
      raf = 0
      window.removeEventListener('keydown', onGestureKey, true)
    }
  })
</script>

<!-- role="application" is the keyboard-driven timeline surface required by ui-spec §7; Svelte does not
     count it as interactive, so the two rules are suppressed on this element only. -->
<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
<div
  class="lane-surface"
  tabindex="0"
  role="application"
  aria-label="Timeline"
  aria-describedby={HINT_ID}
  onpointerdown={onPointerDown}
  onpointermove={onPointerMove}
  onpointerup={onPointerUp}
  onpointercancel={cancelGesture}
  onpointerleave={onPointerLeave}
  onkeydown={onKeyDown}
  onkeyup={onKeyUp}
>
  <canvas bind:this={canvasEl} class="lane-canvas" aria-hidden="true"></canvas>
  <span id={HINT_ID} class="sr-only">{KEYS_HINT}</span>
</div>

<style>
  .lane-surface {
    position: absolute;
    inset: 0;
    outline-offset: -2px;
    touch-action: none;
  }

  .lane-canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
</style>
