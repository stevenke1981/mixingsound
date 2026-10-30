// IPC types. Mirrors crates/engine/src/{model,edit,render,bounce}.rs and src-tauri/src/dto.rs.
// Keep in sync with the Rust side. All positions/lengths are frames at project.sampleRate.

export type TrackId = number
export type ClipId = number
export type SourceId = number

export const MIN_GAIN_DB = -96
export const MAX_TRACK_GAIN_DB = 12
export const MAX_CLIP_GAIN_DB = 24

export interface Clip {
  id: ClipId
  sourceId: SourceId
  start: number
  offset: number
  length: number
  gainDb: number
  fadeIn: number
  fadeOut: number
}

export interface Track {
  id: TrackId
  name: string
  gainDb: number
  pan: number
  mute: boolean
  solo: boolean
  color: string
  clips: Clip[]
}

export interface SourceRef {
  id: SourceId
  name: string
  path: string
  channels: number
  frames: number
  originalSampleRate: number
}

export interface Project {
  version: number
  sampleRate: number
  tracks: Track[]
  master: { gainDb: number }
  sources: SourceRef[]
  nextId: number
}

export type Edit =
  | { type: 'addTrack'; name: string; index: number | null }
  | { type: 'removeTrack'; track: TrackId }
  | { type: 'renameTrack'; track: TrackId; name: string }
  | { type: 'moveTrack'; track: TrackId; index: number }
  | { type: 'setTrackGain'; track: TrackId; gainDb: number }
  | { type: 'setTrackPan'; track: TrackId; pan: number }
  | { type: 'setTrackMute'; track: TrackId; mute: boolean }
  | { type: 'setTrackSolo'; track: TrackId; solo: boolean }
  | { type: 'setTrackColor'; track: TrackId; color: string }
  | { type: 'setMasterGain'; gainDb: number }
  | { type: 'addClip'; track: TrackId; source: SourceId; start: number }
  | { type: 'removeClip'; clip: ClipId }
  | { type: 'moveClip'; clip: ClipId; track: TrackId; start: number }
  | { type: 'trimClip'; clip: ClipId; start: number; offset: number; length: number }
  | { type: 'splitClip'; clip: ClipId; at: number }
  | { type: 'setClipFades'; clip: ClipId; fadeIn: number; fadeOut: number }
  | { type: 'setClipGain'; clip: ClipId; gainDb: number }

export interface AppSnapshot {
  project: Project
  canUndo: boolean
  canRedo: boolean
  projectPath: string | null
  dirty: boolean
  deviceName: string | null
  missingSources: SourceId[]
  warnings: string[]
}

/** Linear values (not dB). */
export interface StereoLevel {
  peakL: number
  peakR: number
  rmsL: number
  rmsR: number
}

/** `engine-status` event payload, ~30 Hz. */
export interface EngineStatus {
  playing: boolean
  position: number
  master: StereoLevel
  tracks: { id: TrackId; level: StereoLevel }[]
  clipped: boolean
  error: string | null
}

export type BitDepth = 'int16' | 'int24' | 'float32'

export interface BounceOptions {
  start: number
  end: number
  bitDepth: BitDepth
  dither: boolean
}

export interface BounceReport {
  path: string
  frames: number
  peakDb: number
  clippedSamples: number
}

/** `[min, max]` per column. */
export type WaveformColumns = [number, number][]
