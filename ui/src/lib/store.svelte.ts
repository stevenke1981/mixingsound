// Central app store (Svelte 5 runes). Contract: docs/design/ui-spec.md §6.2.
// Components call `store.x()` directly (never detach methods); actions never throw.
// Additions beyond the skeleton (all public, listed in the WP-A report): `AppStore` class export,
// `confirmRequest` + `askConfirm` / `resolveConfirm` (in-app dialog), `removeSelectedTrack`.

import { api, errorMessage, onEngineStatus, onExportProgress } from './api'
import {
  clampFpp,
  defaultFpp,
  fitFpp,
  zoomAt,
} from './geometry'
import {
  classifyPaths,
  pickAudioFiles,
  pickExportPath,
  pickProjectFile,
  pickProjectSavePath,
  subscribeFileDrop,
} from './os'
import { fileStem } from './format'
import {
  MAX_CLIP_GAIN_DB,
  MAX_TRACK_GAIN_DB,
  MIN_GAIN_DB,
  type AppSnapshot,
  type BitDepth,
  type BounceReport,
  type Clip,
  type Edit,
  type EngineStatus,
  type Project,
  type SourceRef,
  type StereoLevel,
  type Track,
} from './types'

export type ToastKind = 'info' | 'warn' | 'error'
export interface Toast {
  id: number
  kind: ToastKind
  text: string
  ttlMs: number | null
}
/** Frames, start < end. */
export interface LoopRegion {
  start: number
  end: number
}
export interface ViewState {
  /** Frames per pixel. */
  fpp: number
  /** Timeline frame at the left edge, ≥ 0. */
  scrollFrames: number
  viewportW: number
  viewportH: number
  follow: boolean
  mixerOpen: boolean
  /** 176..340, default 200. */
  mixerHeight: number
}
export interface ExportUi {
  open: boolean
  phase: 'form' | 'running' | 'done' | 'error'
  progress: number
  report: BounceReport | null
  error: string | null
}
export interface ExportRequest {
  range: 'project' | 'loop'
  bitDepth: BitDepth
  dither: boolean
}
export type LatchKey = number | 'master'

export interface ConfirmButton {
  id: string
  label: string
  tone?: 'normal' | 'primary' | 'danger'
}
export interface ConfirmRequest {
  title: string
  message: string
  buttons: ConfirmButton[]
}
export interface PendingConfirm {
  request: ConfirmRequest
  /** Resolves with the chosen button id, or `null` when dismissed (Esc). */
  resolve: (id: string | null) => void
}

const ZERO_LEVEL: StereoLevel = { peakL: 0, peakR: 0, rmsL: 0, rmsR: 0 }
const IDLE_STATUS: EngineStatus = {
  playing: false,
  position: 0,
  master: ZERO_LEVEL,
  tracks: [],
  clipped: false,
  error: null,
}

const TOAST_TTL_MS: Record<ToastKind, number | null> = { info: 4000, warn: 7000, error: null }
const MAX_TOASTS = 4
const TOAST_DEDUPE_MS = 2000
/** Engine position is ignored this long after a seek so the playhead does not snap back. */
const SEEK_HOLD_MS = 300
/** Peak at or above this (linear) latches the clip LED. */
const CLIP_PEAK = 0.9999
const FOLLOW_EDGE = 0.9
const FOLLOW_LEAD = 0.1
const MIXER_MIN_H = 176
const MIXER_MAX_H = 340
const MIXER_MAX_WINDOW_FRACTION = 0.55
const DEFAULT_WINDOW_H = 900
const LOOP_FROM_PLAYHEAD_SECONDS = 4
const DEFAULT_SAMPLE_RATE = 48000
const LIVE_SEPARATOR = ':'

type BusyKind = 'import' | 'open' | 'save' | 'export'

interface LiveSlot {
  inFlight: boolean
  pending: Edit | null
  run: Promise<void> | null
}

function clamp(value: number, lo: number, hi: number): number {
  if (!Number.isFinite(value)) return lo
  return Math.min(hi, Math.max(lo, value))
}

function frameOf(value: number): number {
  return Number.isFinite(value) ? Math.max(0, Math.round(value)) : 0
}

function clampMixerHeight(px: number): number {
  const windowH = typeof window !== 'undefined' ? window.innerHeight : DEFAULT_WINDOW_H
  const maxForWindow = Math.floor(windowH * MIXER_MAX_WINDOW_FRACTION)
  return clamp(px, MIXER_MIN_H, Math.min(MIXER_MAX_H, Math.max(MIXER_MIN_H, maxForWindow)))
}

function clipIdsOf(project: Project | null): number[] {
  return (project?.tracks ?? []).flatMap((t) => t.clips.map((c) => c.id))
}

function peakOf(level: StereoLevel): number {
  return Math.max(level.peakL, level.peakR)
}

export class AppStore {
  // ---- reactive state (read-only to components; mutate only through actions) ----
  snapshot = $state.raw<AppSnapshot | null>(null)
  status = $state.raw<EngineStatus>(IDLE_STATUS)
  position = $state(0)
  loop = $state<LoopRegion | null>(null)
  loopEnabled = $state(false)
  selection = $state<{ clipId: number | null; trackId: number | null }>({
    clipId: null,
    trackId: null,
  })
  view = $state<ViewState>({
    fpp: 480,
    scrollFrames: 0,
    viewportW: 0,
    viewportH: 0,
    follow: true,
    mixerOpen: true,
    mixerHeight: 200,
  })
  toasts = $state<Toast[]>([])
  busy = $state<null | BusyKind>(null)
  dropActive = $state(false)
  dropCount = $state(0)
  export = $state<ExportUi>({ open: false, phase: 'form', progress: 0, report: null, error: null })
  clipLatch = $state<Record<LatchKey, boolean>>({} as Record<LatchKey, boolean>)
  projectEpoch = $state(0)
  announcement = $state('')
  confirmRequest = $state.raw<PendingConfirm | null>(null)

  // ---- non-reactive bookkeeping ----
  private readonly subscriptions: Promise<() => void>[] = []
  private readonly live = new Map<string, LiveSlot>()
  private readonly toastTimers = new Map<number, ReturnType<typeof setTimeout>>()
  private readonly recentToasts = new Map<string, number>()
  private toastSeq = 0
  private seekHoldUntil = 0
  private lastStatusError: string | null = null

  // ---- derived getters ----
  get project(): Project | null {
    return this.snapshot?.project ?? null
  }
  get dirty(): boolean {
    return this.snapshot?.dirty ?? false
  }
  get canUndo(): boolean {
    return this.snapshot?.canUndo ?? false
  }
  get canRedo(): boolean {
    return this.snapshot?.canRedo ?? false
  }
  get playing(): boolean {
    return this.status.playing
  }
  get hasDevice(): boolean {
    return this.snapshot?.deviceName != null
  }
  get sampleRate(): number {
    return this.project?.sampleRate ?? DEFAULT_SAMPLE_RATE
  }
  get projectEnd(): number {
    let end = 0
    for (const t of this.project?.tracks ?? []) {
      for (const c of t.clips) end = Math.max(end, c.start + c.length)
    }
    return end
  }
  get anySolo(): boolean {
    return this.project?.tracks.some((t) => t.solo) ?? false
  }
  trackById(id: number): Track | undefined {
    return this.project?.tracks.find((t) => t.id === id)
  }
  clipById(id: number): { clip: Clip; track: Track } | undefined {
    for (const track of this.project?.tracks ?? []) {
      const clip = track.clips.find((c) => c.id === id)
      if (clip) return { clip, track }
    }
    return undefined
  }
  sourceById(id: number): SourceRef | undefined {
    return this.project?.sources.find((s) => s.id === id)
  }
  isMissing(sourceId: number): boolean {
    return this.snapshot?.missingSources.includes(sourceId) ?? false
  }
  levelOf(trackId: number): StereoLevel | undefined {
    return this.status.tracks.find((t) => t.id === trackId)?.level
  }

  // ---- lifecycle ----
  async init(): Promise<void> {
    if (this.subscriptions.length > 0) return
    this.subscriptions.push(
      onEngineStatus((s) => this.onStatus(s)),
      onExportProgress((f) => this.onExportProgress(f)),
      subscribeFileDrop({
        enter: (count) => this.setDrop(true, count),
        leave: () => this.setDrop(false, 0),
        drop: (paths) => {
          this.setDrop(false, 0)
          void this.runAction(() => this.handleDrop(paths))
        },
      }),
    )
    await Promise.all(this.subscriptions.map((p) => p.catch((e: unknown) => this.fail(e))))
    await this.runAction(async () => this.loadSnapshot(await api.init(), true))
  }

  dispose(): void {
    for (const p of this.subscriptions) void p.then((off) => off(), () => undefined)
    this.subscriptions.length = 0
    for (const timer of this.toastTimers.values()) clearTimeout(timer)
    this.toastTimers.clear()
    this.live.clear()
  }

  // ---- project ----
  async newProject(): Promise<void> {
    if (!(await this.confirmDiscard())) return
    await this.runBusy('open', async () => {
      this.loadSnapshot(await api.newProject(), true)
      await this.resetEngineLoop()
    })
  }

  async openDialog(): Promise<void> {
    await this.runAction(async () => {
      const path = await pickProjectFile()
      if (path !== null && (await this.confirmDiscard())) await this.loadProjectFile(path)
    })
  }

  async openPath(path: string): Promise<void> {
    await this.runAction(async () => {
      if (await this.confirmDiscard()) await this.loadProjectFile(path)
    })
  }

  async save(): Promise<void> {
    if (!this.snapshot) return
    if (this.snapshot.projectPath === null) {
      await this.saveAs()
      return
    }
    if (!this.dirty) return
    await this.runBusy('save', async () => {
      this.applySnapshot(await api.saveProject(null))
    })
  }

  async saveAs(): Promise<void> {
    if (!this.snapshot) return
    await this.runAction(async () => {
      const path = await pickProjectSavePath(`${this.projectStem()}.msproj`)
      if (path === null) return
      await this.runBusy('save', async () => {
        this.applySnapshot(await api.saveProject(path))
      })
    })
  }

  async importDialog(): Promise<void> {
    await this.runAction(async () => {
      await this.importPaths(await pickAudioFiles())
    })
  }

  async importPaths(paths: string[]): Promise<void> {
    if (paths.length === 0) return
    await this.runBusy('import', async () => {
      this.applySnapshot(await api.importFiles(paths))
    })
  }

  async undo(): Promise<void> {
    await this.runAction(async () => this.applySnapshot(await api.undo()))
  }

  async redo(): Promise<void> {
    await this.runAction(async () => this.applySnapshot(await api.redo()))
  }

  // ---- transport ----
  async play(): Promise<void> {
    if (!this.requireDevice()) return
    this.patchView({ follow: true })
    this.status = { ...this.status, playing: true }
    await this.runAction(() => api.play())
  }

  async pause(): Promise<void> {
    this.status = { ...this.status, playing: false }
    await this.runAction(() => api.pause())
  }

  async togglePlay(): Promise<void> {
    if (this.playing) await this.pause()
    else await this.play()
  }

  async stop(): Promise<void> {
    this.status = { ...this.status, playing: false }
    await this.runAction(() => api.stop())
  }

  async returnToStart(): Promise<void> {
    await this.seek(0)
  }

  /** Optimistic: sets `position` first. */
  async seek(frame: number): Promise<void> {
    const target = frameOf(frame)
    this.position = target
    this.seekHoldUntil = Date.now() + SEEK_HOLD_MS
    await this.runAction(() => api.seek(target))
  }

  /** Also enables looping. */
  async setLoopRegion(start: number, end: number): Promise<void> {
    const region = { start: frameOf(start), end: frameOf(end) }
    if (region.end <= region.start) {
      this.toast('warn', 'Loop region is too short')
      return
    }
    this.loop = region
    this.loopEnabled = true
    await this.runAction(() => api.setLoop(region.start, region.end))
  }

  async toggleLoop(): Promise<void> {
    const region = this.loop
    if (region === null) {
      const fallback = this.defaultLoopRegion()
      await this.setLoopRegion(fallback.start, fallback.end)
      return
    }
    const enabled = !this.loopEnabled
    this.loopEnabled = enabled
    await this.runAction(() =>
      api.setLoop(enabled ? region.start : null, enabled ? region.end : null),
    )
  }

  async clearLoop(): Promise<void> {
    this.loop = null
    this.loopEnabled = false
    await this.runAction(() => api.setLoop(null, null))
  }

  // ---- edits ----
  /** One undo step. Resolves false on error (already toasted). */
  async edit(e: Edit): Promise<boolean> {
    try {
      this.applySnapshot(await api.applyEdit(e))
      return true
    } catch (err) {
      this.fail(err)
      return false
    }
  }

  /** Throttled: one request in flight per key, newest value wins. */
  liveEdit(key: string, e: Edit): void {
    const slot = this.liveSlot(key)
    slot.pending = e
    if (!slot.inFlight) this.startLive(slot)
  }

  /** Flushes pending live edits, then `api.endGesture`. */
  async endGesture(): Promise<void> {
    await this.drainAllLive()
    await this.runAction(() => api.endGesture())
  }

  async addTrack(): Promise<void> {
    const name = `Track ${(this.project?.tracks.length ?? 0) + 1}`
    if (!(await this.edit({ type: 'addTrack', name, index: null }))) return
    const added = this.project?.tracks.at(-1)
    if (added) this.selectTrack(added.id)
  }

  async removeTrack(id: number): Promise<void> {
    if (await this.edit({ type: 'removeTrack', track: id })) {
      this.toast('info', 'Track removed – Ctrl+Z to undo')
    }
  }

  /** Confirms, then removes the selected track. */
  async removeSelectedTrack(): Promise<void> {
    const track = this.selection.trackId === null ? undefined : this.trackById(this.selection.trackId)
    if (!track) return
    const choice = await this.askConfirm({
      title: `Remove track “${track.name}”?`,
      message: 'Its clips are removed too. Ctrl+Z undoes this.',
      buttons: [
        { id: 'cancel', label: 'Cancel' },
        { id: 'remove', label: 'Remove track', tone: 'danger' },
      ],
    })
    if (choice === 'remove') await this.removeTrack(track.id)
  }

  async renameTrack(id: number, name: string): Promise<void> {
    const trimmed = name.trim()
    if (trimmed === '' || this.trackById(id)?.name === trimmed) return
    await this.edit({ type: 'renameTrack', track: id, name: trimmed })
  }

  async moveTrack(id: number, index: number): Promise<void> {
    await this.edit({ type: 'moveTrack', track: id, index: Math.max(0, Math.round(finiteOr(index))) })
  }

  async setTrackColor(id: number, color: string): Promise<void> {
    await this.edit({ type: 'setTrackColor', track: id, color })
  }

  async setTrackMute(id: number, mute: boolean): Promise<void> {
    await this.edit({ type: 'setTrackMute', track: id, mute })
  }

  async setTrackSolo(id: number, solo: boolean): Promise<void> {
    await this.edit({ type: 'setTrackSolo', track: id, solo })
  }

  setTrackGain(id: number, db: number): void {
    const gainDb = clamp(db, MIN_GAIN_DB, MAX_TRACK_GAIN_DB)
    this.liveEdit(`track-gain${LIVE_SEPARATOR}${id}`, { type: 'setTrackGain', track: id, gainDb })
  }

  setTrackPan(id: number, pan: number): void {
    const clamped = clamp(pan, -1, 1)
    this.liveEdit(`track-pan${LIVE_SEPARATOR}${id}`, { type: 'setTrackPan', track: id, pan: clamped })
  }

  setMasterGain(db: number): void {
    const gainDb = clamp(db, MIN_GAIN_DB, MAX_TRACK_GAIN_DB)
    this.liveEdit('master-gain', { type: 'setMasterGain', gainDb })
  }

  async moveClip(clip: number, track: number, start: number): Promise<void> {
    await this.edit({ type: 'moveClip', clip, track, start: frameOf(start) })
  }

  async trimClip(clip: number, start: number, offset: number, length: number): Promise<void> {
    await this.edit({
      type: 'trimClip',
      clip,
      start: frameOf(start),
      offset: frameOf(offset),
      length: frameOf(length),
    })
  }

  async setClipFades(clip: number, fadeIn: number, fadeOut: number): Promise<void> {
    await this.edit({ type: 'setClipFades', clip, fadeIn: frameOf(fadeIn), fadeOut: frameOf(fadeOut) })
  }

  async setClipGain(clip: number, db: number): Promise<void> {
    const gainDb = clamp(db, MIN_GAIN_DB, MAX_CLIP_GAIN_DB)
    await this.edit({ type: 'setClipGain', clip, gainDb })
  }

  async splitClip(clip: number, at: number): Promise<void> {
    await this.edit({ type: 'splitClip', clip, at: frameOf(at) })
  }

  async removeClip(clip: number): Promise<void> {
    await this.edit({ type: 'removeClip', clip })
  }

  /**
   * Splits the selected clip (or, with no clip selected, the clip under the playhead on the
   * selected track) at the playhead. The backend returns no new id, so the right half is found
   * by diffing clip ids and then selected.
   */
  async splitSelectedAtPlayhead(): Promise<void> {
    const clip = this.splitTarget()
    if (!clip) {
      this.toast('warn', 'Playhead is not inside a clip to split')
      return
    }
    const before = new Set(clipIdsOf(this.project))
    if (!(await this.edit({ type: 'splitClip', clip: clip.id, at: this.position }))) return
    const created = clipIdsOf(this.project).find((id) => !before.has(id))
    if (created !== undefined) this.selectClip(created)
  }

  async deleteSelection(): Promise<void> {
    const id = this.selection.clipId
    if (id === null) return
    await this.removeClip(id)
  }

  // ---- selection / view ----
  /** Also selects the clip's track. */
  selectClip(clipId: number | null): void {
    const found = clipId === null ? undefined : this.clipById(clipId)
    this.selection = {
      clipId: found?.clip.id ?? null,
      trackId: found?.track.id ?? this.selection.trackId,
    }
  }

  selectTrack(trackId: number | null): void {
    const owner = this.selection.clipId === null ? undefined : this.clipById(this.selection.clipId)?.track.id
    this.selection = {
      trackId,
      clipId: owner === trackId ? this.selection.clipId : null,
    }
  }

  setViewport(w: number, h: number): void {
    this.patchView({ viewportW: Math.max(0, finiteOr(w)), viewportH: Math.max(0, finiteOr(h)) })
  }

  zoomBy(factor: number, anchorPx?: number): void {
    if (!Number.isFinite(factor) || factor <= 0) return
    const anchor = anchorPx ?? this.view.viewportW / 2
    // zoomAt scales fpp by its factor (>1 zooms out); zoomBy's factor is zoom-in (>1 zooms in).
    const zoomed = zoomAt(this.view.fpp, this.view.scrollFrames, 1 / factor, anchor, this.sampleRate)
    this.patchView({ fpp: zoomed.fpp, scrollFrames: zoomed.scrollFrames })
  }

  setFpp(fpp: number): void {
    if (!Number.isFinite(fpp)) return
    this.patchView({ fpp: clampFpp(fpp, this.sampleRate) })
  }

  fitProject(): void {
    if (this.view.viewportW <= 0) return
    this.patchView({ fpp: fitFpp(this.projectEnd, this.view.viewportW, this.sampleRate), scrollFrames: 0 })
  }

  /** Public scroll calls are manual: while playing they release follow-scroll. */
  scrollToFrame(frame: number): void {
    this.releaseFollowIfPlaying()
    this.setScroll(frame)
  }

  scrollByPx(dx: number): void {
    this.releaseFollowIfPlaying()
    this.setScroll(this.view.scrollFrames + finiteOr(dx) * this.view.fpp)
  }

  toggleFollow(): void {
    this.patchView({ follow: !this.view.follow })
  }

  toggleMixer(): void {
    this.patchView({ mixerOpen: !this.view.mixerOpen })
  }

  setMixerHeight(px: number): void {
    this.patchView({ mixerHeight: clampMixerHeight(px) })
  }

  clearClip(id: LatchKey): void {
    if (!this.clipLatch[id]) return
    this.clipLatch = { ...this.clipLatch, [id]: false }
  }

  // ---- export ----
  openExport(): void {
    this.export = { open: true, phase: 'form', progress: 0, report: null, error: null }
  }

  closeExport(): void {
    if (this.export.phase === 'running') return
    this.export = { ...this.export, open: false }
  }

  async runExport(o: ExportRequest): Promise<void> {
    if (this.export.phase === 'running') return
    const range = this.exportRange(o.range)
    if (range === null) {
      this.failExport('Nothing to export – add clips or set a loop region')
      return
    }
    let path: string | null
    try {
      path = await pickExportPath(`${this.projectStem()}.wav`)
    } catch (err) {
      this.failExport(errorMessage(err))
      return
    }
    if (path === null) return
    const target = path
    this.export = { open: true, phase: 'running', progress: 0, report: null, error: null }
    await this.runBusy('export', async () => {
      try {
        const report = await api.exportWav(target, {
          start: range.start,
          end: range.end,
          bitDepth: o.bitDepth,
          dither: o.dither,
        })
        this.export = { ...this.export, phase: 'done', progress: 1, report }
        this.announce('Export complete')
      } catch (err) {
        this.failExport(errorMessage(err))
      }
    })
  }

  // ---- in-app confirmation ----
  /** Shows ConfirmDialog; resolves with the chosen button id, or `null` when dismissed. */
  askConfirm(request: ConfirmRequest): Promise<string | null> {
    this.confirmRequest?.resolve(null)
    return new Promise((resolve) => {
      this.confirmRequest = { request, resolve }
    })
  }

  resolveConfirm(id: string | null): void {
    const current = this.confirmRequest
    if (!current) return
    this.confirmRequest = null
    current.resolve(id)
  }

  // ---- feedback ----
  toast(kind: ToastKind, text: string): void {
    const now = Date.now()
    this.pruneRecentToasts(now)
    const key = `${kind}${LIVE_SEPARATOR}${text}`
    const last = this.recentToasts.get(key)
    if (last !== undefined && now - last < TOAST_DEDUPE_MS) return
    this.recentToasts.set(key, now)

    const ttlMs = TOAST_TTL_MS[kind]
    const id = ++this.toastSeq
    const next = [...this.toasts, { id, kind, text, ttlMs }]
    for (const dropped of next.slice(0, -MAX_TOASTS)) this.clearToastTimer(dropped.id)
    this.toasts = next.slice(-MAX_TOASTS)
    if (ttlMs !== null) {
      this.toastTimers.set(id, setTimeout(() => this.dismissToast(id), ttlMs))
    }
  }

  dismissToast(id: number): void {
    this.clearToastTimer(id)
    this.toasts = this.toasts.filter((t) => t.id !== id)
  }

  announce(text: string): void {
    this.announcement = text
  }

  // ================= private =================

  private async runAction(task: () => Promise<void>): Promise<void> {
    try {
      await task()
    } catch (err) {
      this.fail(err)
    }
  }

  private async runBusy(kind: BusyKind, task: () => Promise<void>): Promise<void> {
    this.busy = kind
    try {
      await task()
    } catch (err) {
      this.fail(err)
    } finally {
      this.busy = null
    }
  }

  private fail(err: unknown): void {
    this.toast('error', errorMessage(err))
  }

  private requireDevice(): boolean {
    if (this.hasDevice) return true
    this.toast('warn', 'No audio output device')
    return false
  }

  private patchView(patch: Partial<ViewState>): void {
    // Skip identical patches: effects that call setViewport would otherwise re-trigger forever.
    const view = this.view
    const keys = Object.keys(patch) as (keyof ViewState)[]
    if (keys.every((k) => view[k] === patch[k])) return
    this.view = { ...view, ...patch }
  }

  private setScroll(frame: number): void {
    const { fpp, viewportW } = this.view
    const max = Math.max(0, this.projectEnd + 30 * this.sampleRate - viewportW * fpp)
    this.patchView({ scrollFrames: Math.min(max, Math.max(0, finiteOr(frame))) })
  }

  private releaseFollowIfPlaying(): void {
    if (this.playing) this.patchView({ follow: false })
  }

  private setDrop(active: boolean, count: number): void {
    this.dropActive = active
    this.dropCount = count
  }

  private projectStem(): string {
    const path = this.snapshot?.projectPath ?? null
    const stem = path === null ? '' : fileStem(path)
    return stem === '' ? 'Untitled' : stem
  }

  private defaultLoopRegion(): LoopRegion {
    const clipId = this.selection.clipId
    const found = clipId === null ? undefined : this.clipById(clipId)
    if (found) return { start: found.clip.start, end: found.clip.start + found.clip.length }
    return {
      start: this.position,
      end: this.position + LOOP_FROM_PLAYHEAD_SECONDS * this.sampleRate,
    }
  }

  /** The clip the split key acts on: selected clip, else the selected track's clip under the playhead. */
  private splitTarget(): Clip | null {
    const selectedId = this.selection.clipId
    const candidates = selectedId !== null
      ? [this.clipById(selectedId)?.clip]
      : (this.trackById(this.selection.trackId ?? -1)?.clips ?? [])
    const pos = this.position
    return (
      candidates.find((c): c is Clip => c !== undefined && pos > c.start && pos < c.start + c.length) ??
      null
    )
  }

  private exportRange(range: 'project' | 'loop'): { start: number; end: number } | null {
    const region = range === 'loop' ? this.loop : { start: 0, end: this.projectEnd }
    return region !== null && region.end > region.start ? { start: region.start, end: region.end } : null
  }

  private failExport(message: string): void {
    this.export = { ...this.export, open: true, phase: 'error', error: message }
    this.toast('error', message)
  }

  private async confirmDiscard(): Promise<boolean> {
    if (!this.dirty) return true
    const choice = await this.askConfirm({
      title: 'Discard unsaved changes?',
      message: 'Changes since the last save will be lost.',
      buttons: [
        { id: 'cancel', label: 'Cancel' },
        { id: 'discard', label: 'Discard', tone: 'danger' },
        { id: 'save', label: 'Save first', tone: 'primary' },
      ],
    })
    if (choice === 'save') {
      await this.save()
      return !this.dirty
    }
    return choice === 'discard'
  }

  private async loadProjectFile(path: string): Promise<void> {
    await this.runBusy('open', async () => {
      const snap = await api.openProject(path)
      this.loadSnapshot(snap, true)
      await this.resetEngineLoop()
      if (snap.missingSources.length > 0) {
        this.toast('warn', `${snap.missingSources.length} missing source file(s) – their clips are silent`)
      }
    })
  }

  private async resetEngineLoop(): Promise<void> {
    this.loop = null
    this.loopEnabled = false
    await api.setLoop(null, null)
  }

  private async handleDrop(paths: string[]): Promise<void> {
    const { audio, projects, skipped } = classifyPaths(paths)
    if (audio.length === 0 && projects.length === 1 && skipped.length === 0) {
      await this.openPath(projects[0] ?? '')
      return
    }
    if (audio.length > 0) await this.importPaths(audio)
    const unsupported = skipped.length + projects.length
    if (unsupported > 0) this.toast('warn', `Skipped ${unsupported} unsupported file(s)`)
  }

  private onStatus(status: EngineStatus): void {
    this.status = status
    if (Date.now() >= this.seekHoldUntil) this.position = status.position
    if (status.error !== null && status.error !== this.lastStatusError) {
      this.toast('error', status.error)
    }
    this.lastStatusError = status.error
    this.latchClips(status)
    this.followPlayhead()
  }

  private onExportProgress(fraction: number): void {
    if (this.export.phase !== 'running') return
    this.export = { ...this.export, progress: clamp(fraction, 0, 1) }
  }

  private latchClips(status: EngineStatus): void {
    const hot: LatchKey[] = status.tracks.filter((t) => peakOf(t.level) >= CLIP_PEAK).map((t) => t.id)
    if (status.clipped || peakOf(status.master) >= CLIP_PEAK) hot.push('master')
    const fresh = hot.filter((key) => !this.clipLatch[key])
    if (fresh.length === 0) return
    const next = { ...this.clipLatch }
    for (const key of fresh) next[key] = true
    this.clipLatch = next
  }

  private followPlayhead(): void {
    const { follow, viewportW, fpp, scrollFrames } = this.view
    if (!this.status.playing || !follow || viewportW <= 0) return
    const span = viewportW * fpp
    const inView = this.position >= scrollFrames && this.position <= scrollFrames + FOLLOW_EDGE * span
    if (!inView) this.setScroll(this.position - FOLLOW_LEAD * span)
  }

  private loadSnapshot(snap: AppSnapshot, fresh: boolean): void {
    if (fresh) {
      this.projectEpoch += 1
      this.clipLatch = {} as Record<LatchKey, boolean>
      this.selection = { clipId: null, trackId: null }
      this.patchView({ fpp: defaultFpp(snap.project.sampleRate), scrollFrames: 0 })
    }
    this.applySnapshot(snap)
  }

  private applySnapshot(next: AppSnapshot): void {
    const previous = new Set(this.snapshot?.warnings ?? [])
    this.snapshot = next
    for (const warning of next.warnings) {
      if (!previous.has(warning)) this.toast('warn', warning)
    }
    this.dropStaleSelection()
  }

  private dropStaleSelection(): void {
    const { clipId, trackId } = this.selection
    const trackOk = trackId === null || this.trackById(trackId) !== undefined
    const clipOk = clipId === null || this.clipById(clipId) !== undefined
    if (trackOk && clipOk) return
    this.selection = { clipId: clipOk ? clipId : null, trackId: trackOk ? trackId : null }
  }

  private liveSlot(key: string): LiveSlot {
    const existing = this.live.get(key)
    if (existing) return existing
    const slot: LiveSlot = { inFlight: false, pending: null, run: null }
    this.live.set(key, slot)
    return slot
  }

  private startLive(slot: LiveSlot): void {
    slot.inFlight = true
    slot.run = this.drainLive(slot)
  }

  /** Sends the newest pending value until none is left. Responses superseded by a newer value are not applied. */
  private async drainLive(slot: LiveSlot): Promise<void> {
    try {
      while (slot.pending !== null) {
        const next: Edit = slot.pending
        slot.pending = null
        const snap = await api.applyEdit(next)
        if (slot.pending === null) this.applySnapshot(snap)
      }
    } catch (err) {
      slot.pending = null
      this.fail(err)
    } finally {
      slot.inFlight = false
      slot.run = null
    }
  }

  private async drainAllLive(): Promise<void> {
    for (;;) {
      const runs = [...this.live.values()].flatMap((s) => (s.inFlight && s.run ? [s.run] : []))
      if (runs.length === 0) return
      await Promise.all(runs)
    }
  }

  private pruneRecentToasts(now: number): void {
    for (const [key, at] of this.recentToasts) {
      if (now - at >= TOAST_DEDUPE_MS) this.recentToasts.delete(key)
    }
  }

  private clearToastTimer(id: number): void {
    const timer = this.toastTimers.get(id)
    if (timer === undefined) return
    clearTimeout(timer)
    this.toastTimers.delete(id)
  }
}

function finiteOr(value: number, fallback = 0): number {
  return Number.isFinite(value) ? value : fallback
}

export const store = new AppStore()
