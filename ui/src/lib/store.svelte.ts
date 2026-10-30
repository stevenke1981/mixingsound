// Central app store (Svelte 5 runes). Contract: docs/design/ui-spec.md §6.2.
// SKELETON committed by the controller so WP-B/WP-C can compile; WP-A implements the bodies.
// Components call `store.x()` directly (never detach methods); actions never throw.

import type {
  AppSnapshot,
  BitDepth,
  BounceReport,
  Clip,
  Edit,
  EngineStatus,
  Project,
  SourceRef,
  StereoLevel,
  Track,
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

const ZERO_LEVEL: StereoLevel = { peakL: 0, peakR: 0, rmsL: 0, rmsR: 0 }
const IDLE_STATUS: EngineStatus = {
  playing: false,
  position: 0,
  master: ZERO_LEVEL,
  tracks: [],
  clipped: false,
  error: null,
}

class AppStore {
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
  busy = $state<null | 'import' | 'open' | 'save' | 'export'>(null)
  dropActive = $state(false)
  dropCount = $state(0)
  export = $state<ExportUi>({ open: false, phase: 'form', progress: 0, report: null, error: null })
  clipLatch = $state<Record<LatchKey, boolean>>({} as Record<LatchKey, boolean>)
  projectEpoch = $state(0)
  announcement = $state('')

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
    return this.project?.sampleRate ?? 48000
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
  async init(): Promise<void> {}
  dispose(): void {}
  // ---- project ----
  async newProject(): Promise<void> {}
  async openDialog(): Promise<void> {}
  async openPath(_path: string): Promise<void> {}
  async save(): Promise<void> {}
  async saveAs(): Promise<void> {}
  async importDialog(): Promise<void> {}
  async importPaths(_paths: string[]): Promise<void> {}
  async undo(): Promise<void> {}
  async redo(): Promise<void> {}
  // ---- transport ----
  async play(): Promise<void> {}
  async pause(): Promise<void> {}
  async togglePlay(): Promise<void> {}
  async stop(): Promise<void> {}
  async returnToStart(): Promise<void> {}
  /** Optimistic: sets `position` first. */
  async seek(_frame: number): Promise<void> {}
  /** Also enables looping. */
  async setLoopRegion(_start: number, _end: number): Promise<void> {}
  async toggleLoop(): Promise<void> {}
  async clearLoop(): Promise<void> {}
  // ---- edits ----
  /** One undo step. Resolves false on error (already toasted). */
  async edit(_e: Edit): Promise<boolean> {
    return false
  }
  /** Throttled: one request in flight per key, newest value wins. */
  liveEdit(_key: string, _e: Edit): void {}
  /** Flushes pending live edits, then `api.endGesture`. */
  async endGesture(): Promise<void> {}
  async addTrack(): Promise<void> {}
  async removeTrack(_id: number): Promise<void> {}
  async renameTrack(_id: number, _name: string): Promise<void> {}
  async moveTrack(_id: number, _index: number): Promise<void> {}
  async setTrackColor(_id: number, _color: string): Promise<void> {}
  async setTrackMute(_id: number, _mute: boolean): Promise<void> {}
  async setTrackSolo(_id: number, _solo: boolean): Promise<void> {}
  setTrackGain(_id: number, _db: number): void {}
  setTrackPan(_id: number, _pan: number): void {}
  setMasterGain(_db: number): void {}
  async moveClip(_clip: number, _track: number, _start: number): Promise<void> {}
  async trimClip(_clip: number, _start: number, _offset: number, _length: number): Promise<void> {}
  async setClipFades(_clip: number, _fadeIn: number, _fadeOut: number): Promise<void> {}
  async setClipGain(_clip: number, _db: number): Promise<void> {}
  async splitClip(_clip: number, _at: number): Promise<void> {}
  async removeClip(_clip: number): Promise<void> {}
  async splitSelectedAtPlayhead(): Promise<void> {}
  async deleteSelection(): Promise<void> {}
  // ---- selection / view ----
  /** Also selects the clip's track. */
  selectClip(_clipId: number | null): void {}
  selectTrack(_trackId: number | null): void {}
  setViewport(_w: number, _h: number): void {}
  zoomBy(_factor: number, _anchorPx?: number): void {}
  setFpp(_fpp: number): void {}
  fitProject(): void {}
  scrollToFrame(_frame: number): void {}
  scrollByPx(_dx: number): void {}
  toggleFollow(): void {}
  toggleMixer(): void {}
  setMixerHeight(_px: number): void {}
  clearClip(_id: LatchKey): void {}
  // ---- export ----
  openExport(): void {}
  closeExport(): void {}
  async runExport(_o: ExportRequest): Promise<void> {}
  // ---- feedback ----
  toast(_kind: ToastKind, _text: string): void {}
  dismissToast(_id: number): void {}
  announce(_text: string): void {}
}

export const store = new AppStore()
