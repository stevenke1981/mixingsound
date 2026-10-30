// Clip edit maths (ui-spec §3.1/§5.4). Pure; all clamps live here. Return null when invalid/no-op. STUB — WP-B1 implements.
import type { Clip } from './types'

export const MIN_CLIP_FRAMES = 64

export interface ClipGeom {
  start: number
  offset: number
  length: number
}

/** New start (≥ 0). */
export function computeMove(clip: Clip, deltaFrames: number): number | null {
  return null
}
/** Left trim to `newStart`: keeps offset ≥ 0, length ≥ MIN_CLIP_FRAMES. */
export function computeTrimLeft(clip: Clip, newStart: number): ClipGeom | null {
  return null
}
/** Right trim to `newEnd`: length ≥ MIN_CLIP_FRAMES, offset+length ≤ sourceFrames. */
export function computeTrimRight(clip: Clip, newEnd: number, sourceFrames: number): ClipGeom | null {
  return null
}
/** Clamp fades ≥ 0 and fadeIn+fadeOut ≤ length; `which` side is the one being dragged. */
export function computeFades(clip: Clip, which: 'in' | 'out', frames: number): { fadeIn: number; fadeOut: number } | null {
  return null
}
/** dB = startDb − dy·0.1 (fine: ×0.1), clamped −96…+24, rounded to 0.1. */
export function computeGain(startDb: number, dyPx: number, fine: boolean): number {
  return startDb
}
/** Split point strictly inside (start, start+length) and both halves ≥ 1 frame; else null. */
export function computeSplit(clip: Clip, at: number): number | null {
  return null
}
