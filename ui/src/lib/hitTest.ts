// Lane hit testing (ui-spec §3.1/§5.4). Pure. STUB — WP-B1 implements.
import type { Project } from './types'

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

/**
 * `x` relative to the lane canvas left, `y` in content coordinates (0 = top of first lane).
 * Priority: fade handles (only for `selectedOrHovered` clip) > edges > body. Null if empty lane/no lane.
 */
export function hitTest(
  project: Project,
  x: number,
  y: number,
  scrollFrames: number,
  fpp: number,
  selectedOrHovered: number | null,
): Hit | null {
  return null
}
/** Track index for content y (may be ≥ tracks.length → null). */
export function laneAt(y: number, trackCount: number): number | null {
  return null
}
