// Snapping (ui-spec §3.1). Pure. STUB — WP-B1 implements.
import type { Project } from './types'

export const SNAP_PX = 8

export interface SnapResult {
  frame: number
  /** The candidate snapped to, or null when not snapped. */
  target: number | null
}

/** Candidates: every clip start/end except `excludeClipId`, playhead, loop start/end, 0. Sorted, deduped. */
export function snapCandidates(
  project: Project,
  excludeClipId: number | null,
  playhead: number,
  loop: { start: number; end: number } | null,
): number[] {
  return [0]
}
/** Nearest candidate within `thresholdFrames` wins; `enabled=false` (Alt) → no snap. */
export function snap(candidates: number[], frame: number, thresholdFrames: number, enabled: boolean): SnapResult {
  return { frame, target: null }
}
/** Move snapping: snaps the clip's start or end, whichever is closer; returns the new start. */
export function snapMove(candidates: number[], start: number, length: number, thresholdFrames: number, enabled: boolean): SnapResult {
  return { frame: start, target: null }
}
