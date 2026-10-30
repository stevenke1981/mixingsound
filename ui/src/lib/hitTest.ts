// Lane hit testing (ui-spec §3.1/§5.4). Pure.
import { LANE_H, frameToX } from './geometry'
import type { Clip, Project } from './types'

export type HitKind = 'body' | 'edgeL' | 'edgeR' | 'fadeIn' | 'fadeOut'
export interface Hit {
  kind: HitKind
  clipId: number
  trackId: number
}
export const EDGE_PX = 6
export const FADE_HANDLE_PX = 8
export const HEADER_STRIP_H = 16
export const CLIP_INSET_Y = 4

/** Clip rect height inside a lane (lane minus the inset above and below). */
const CLIP_H = LANE_H - 2 * CLIP_INSET_Y
/** Below this width the edge zone shrinks to a quarter of the clip so edges never overlap. */
const NARROW_CLIP_PX = 24

interface Span {
  clip: Clip
  left: number
  right: number
}

/**
 * `x` relative to the lane canvas left, `y` in content coordinates (0 = top of first lane).
 * Priority: fade handles (only for `selectedOrHovered` clip) > edges > body. Null if empty lane/no lane.
 * Within one priority tier a later clip in the array wins on overlap.
 */
export function hitTest(
  project: Project,
  x: number,
  y: number,
  scrollFrames: number,
  fpp: number,
  selectedOrHovered: number | null,
): Hit | null {
  if (!(fpp > 0) || !Number.isFinite(x) || !Number.isFinite(y)) return null
  const laneIndex = laneAt(y, project.tracks.length)
  const track = laneIndex === null ? undefined : project.tracks[laneIndex]
  if (laneIndex === null || track === undefined) return null

  const top = laneIndex * LANE_H + CLIP_INSET_Y
  if (y < top || y >= top + CLIP_H) return null

  const spans: Span[] = track.clips.map((clip) => ({
    clip,
    left: frameToX(clip.start, scrollFrames, fpp),
    right: frameToX(clip.start + clip.length, scrollFrames, fpp),
  }))
  const ordered = [...spans].reverse()

  return (
    hitFade(ordered, track.id, x, y, top, fpp, selectedOrHovered) ??
    hitEdge(ordered, track.id, x) ??
    hitBody(ordered, track.id, x)
  )
}

/** Track index for content y (may be ≥ tracks.length → null). */
export function laneAt(y: number, trackCount: number): number | null {
  if (!Number.isFinite(y) || y < 0) return null
  const index = Math.floor(y / LANE_H)
  return index < trackCount ? index : null
}

function hitFade(
  spans: Span[],
  trackId: number,
  x: number,
  y: number,
  top: number,
  fpp: number,
  selectedId: number | null,
): Hit | null {
  if (selectedId === null) return null
  const handleTop = top + HEADER_STRIP_H
  if (y < handleTop || y >= handleTop + FADE_HANDLE_PX) return null

  for (const { clip, left, right } of spans) {
    if (clip.id !== selectedId) continue
    if (isInHandle(x, left + clip.fadeIn / fpp)) return { kind: 'fadeIn', clipId: clip.id, trackId }
    if (isInHandle(x, right - clip.fadeOut / fpp)) return { kind: 'fadeOut', clipId: clip.id, trackId }
  }
  return null
}

/** Handle is an 8 px square centred on `centreX`; half-open like the other zones. */
function isInHandle(x: number, centreX: number): boolean {
  const half = FADE_HANDLE_PX / 2
  return x >= centreX - half && x < centreX + half
}

function hitEdge(spans: Span[], trackId: number, x: number): Hit | null {
  for (const { clip, left, right } of spans) {
    if (!containsX(left, right, x)) continue
    const width = right - left
    const edgePx = width < NARROW_CLIP_PX ? width / 4 : EDGE_PX
    if (x - left < edgePx) return { kind: 'edgeL', clipId: clip.id, trackId }
    if (right - x <= edgePx) return { kind: 'edgeR', clipId: clip.id, trackId }
  }
  return null
}

function hitBody(spans: Span[], trackId: number, x: number): Hit | null {
  for (const { clip, left, right } of spans) {
    if (containsX(left, right, x)) return { kind: 'body', clipId: clip.id, trackId }
  }
  return null
}

function containsX(left: number, right: number, x: number): boolean {
  return right > left && x >= left && x < right
}
