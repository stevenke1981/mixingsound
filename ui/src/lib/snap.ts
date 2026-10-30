// Snapping (ui-spec §3.1). Pure.
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
  const found = new Set<number>([0, playhead])
  if (loop) {
    found.add(loop.start)
    found.add(loop.end)
  }
  for (const track of project.tracks) {
    for (const clip of track.clips) {
      if (clip.id === excludeClipId) continue
      found.add(clip.start)
      found.add(clip.start + clip.length)
    }
  }
  return [...found].filter(Number.isFinite).sort((a, b) => a - b)
}

/**
 * Nearest candidate within `thresholdFrames` wins; `enabled=false` (Alt) → no snap.
 * Ties keep the earlier candidate. Does not require `candidates` to be sorted.
 */
export function snap(candidates: number[], frame: number, thresholdFrames: number, enabled: boolean): SnapResult {
  const unsnapped: SnapResult = { frame, target: null }
  if (!enabled || !Number.isFinite(frame)) return unsnapped
  const nearest = nearestWithin(candidates, frame, thresholdFrames)
  return nearest === null ? unsnapped : { frame: nearest, target: nearest }
}

/**
 * Move snapping: snaps the clip's start or end, whichever is closer; returns the new start.
 * Candidates that would push the start below 0 are ignored; the result start is never negative.
 */
export function snapMove(candidates: number[], start: number, length: number, thresholdFrames: number, enabled: boolean): SnapResult {
  const unsnapped: SnapResult = { frame: Math.max(0, start), target: null }
  if (!enabled || !Number.isFinite(start) || !Number.isFinite(length)) return unsnapped

  const startHit = nearestWithin(candidates, start, thresholdFrames)
  const startDistance = startHit === null ? Infinity : Math.abs(startHit - start)
  const endHit = nearestWithin(candidates, start + length, thresholdFrames, (c) => c - length >= 0)
  const endDistance = endHit === null ? Infinity : Math.abs(endHit - (start + length))

  if (startHit !== null && startDistance <= endDistance) {
    return { frame: startHit, target: startHit }
  }
  if (endHit !== null) {
    const newStart = endHit - length
    return { frame: newStart, target: endHit }
  }
  return unsnapped
}

/** Nearest candidate to `value` with |c − value| ≤ threshold, optionally filtered. */
function nearestWithin(
  candidates: number[],
  value: number,
  thresholdFrames: number,
  accept: (candidate: number) => boolean = () => true,
): number | null {
  let best: number | null = null
  let bestDistance = Infinity
  for (const candidate of candidates) {
    if (!Number.isFinite(candidate) || !accept(candidate)) continue
    const distance = Math.abs(candidate - value)
    if (distance <= thresholdFrames && distance < bestDistance) {
      best = candidate
      bestDistance = distance
    }
  }
  return best
}
