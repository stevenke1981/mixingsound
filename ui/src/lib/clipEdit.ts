// Clip edit maths (ui-spec §3.1/§5.4). Pure; all clamps live here. Return null when invalid/no-op.
import { MAX_CLIP_GAIN_DB, MIN_GAIN_DB, type Clip } from './types'

export const MIN_CLIP_FRAMES = 64

/** Pointer gain sensitivity: dB per screen pixel. Shift multiplies it by 0.1. */
const GAIN_DB_PER_PX = 0.1
const FINE_GAIN_FACTOR = 0.1
/** Gain is stored on a 0.1 dB grid; dividing by an integer keeps values free of float noise. */
const GAIN_STEPS_PER_DB = 10

export interface ClipGeom {
  start: number
  offset: number
  length: number
}

function clampTo(value: number, lo: number, hi: number): number {
  return Math.min(hi, Math.max(lo, value))
}

/** New start (≥ 0). */
export function computeMove(clip: Clip, deltaFrames: number): number | null {
  if (!Number.isFinite(deltaFrames)) return null
  const newStart = Math.max(0, Math.round(clip.start + deltaFrames))
  return newStart === clip.start ? null : newStart
}

/**
 * Left trim to `newStart`: keeps offset ≥ 0, length ≥ MIN_CLIP_FRAMES.
 * The clip end stays fixed; `newStart` is clamped into the legal range. Clips already shorter than
 * the minimum cannot be trimmed (clamping would grow them), so the result is null.
 */
export function computeTrimLeft(clip: Clip, newStart: number): ClipGeom | null {
  if (!Number.isFinite(newStart) || clip.length < MIN_CLIP_FRAMES) return null
  const end = clip.start + clip.length
  const lo = Math.max(0, clip.start - clip.offset)
  const hi = end - MIN_CLIP_FRAMES
  if (hi < lo) return null
  const start = clampTo(Math.round(newStart), lo, hi)
  if (start === clip.start) return null
  return {
    start,
    offset: clip.offset + (start - clip.start),
    length: end - start,
  }
}

/**
 * Right trim to `newEnd`: length ≥ MIN_CLIP_FRAMES, offset+length ≤ sourceFrames.
 * `newEnd` is clamped into the legal range; start and offset are unchanged.
 */
export function computeTrimRight(clip: Clip, newEnd: number, sourceFrames: number): ClipGeom | null {
  if (!Number.isFinite(newEnd) || !Number.isFinite(sourceFrames) || clip.length < MIN_CLIP_FRAMES) return null
  const lo = clip.start + MIN_CLIP_FRAMES
  const hi = clip.start + (sourceFrames - clip.offset)
  if (hi < lo) return null
  const end = clampTo(Math.round(newEnd), lo, hi)
  const length = end - clip.start
  if (length === clip.length) return null
  return { start: clip.start, offset: clip.offset, length }
}

/**
 * Clamp fades ≥ 0 and fadeIn+fadeOut ≤ length; `which` side is the one being dragged.
 * `frames` is the requested fade length on that side; the other side is kept as-is.
 */
export function computeFades(clip: Clip, which: 'in' | 'out', frames: number): { fadeIn: number; fadeOut: number } | null {
  if (!Number.isFinite(frames)) return null
  const requested = Math.max(0, Math.round(frames))
  const next =
    which === 'in'
      ? { fadeIn: Math.min(requested, Math.max(0, clip.length - clip.fadeOut)), fadeOut: clip.fadeOut }
      : { fadeIn: clip.fadeIn, fadeOut: Math.min(requested, Math.max(0, clip.length - clip.fadeIn)) }
  if (next.fadeIn === clip.fadeIn && next.fadeOut === clip.fadeOut) return null
  return next
}

/** dB = startDb − dy·0.1 (fine: ×0.1), clamped −96…+24, rounded to 0.1. */
export function computeGain(startDb: number, dyPx: number, fine: boolean): number {
  const base = Number.isFinite(startDb) ? startDb : 0
  const dy = Number.isFinite(dyPx) ? dyPx : 0
  const factor = fine ? GAIN_DB_PER_PX * FINE_GAIN_FACTOR : GAIN_DB_PER_PX
  const rounded = Math.round((base - dy * factor) * GAIN_STEPS_PER_DB) / GAIN_STEPS_PER_DB
  const clamped = clampTo(rounded, MIN_GAIN_DB, MAX_CLIP_GAIN_DB)
  // Normalise −0 so equality checks and display do not see a negative zero.
  return clamped === 0 ? 0 : clamped
}

/** Split point strictly inside (start, start+length) and both halves ≥ 1 frame; else null. */
export function computeSplit(clip: Clip, at: number): number | null {
  if (!Number.isFinite(at)) return null
  const split = Math.round(at)
  const end = clip.start + clip.length
  if (split - clip.start < 1 || end - split < 1) return null
  return split
}
