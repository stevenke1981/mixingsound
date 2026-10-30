// Timeline geometry (ui-spec §5.1). Pure: plain numbers in, plain numbers out. STUB — WP-B1 implements.

export const LANE_H = 72
export const RULER_H = 28
export const HEADER_W = 208
/** Zoom step per Ctrl+wheel notch / zoom button. */
export const ZOOM_STEP = 1.25

export function defaultFpp(sampleRate: number): number {
  return sampleRate / 100
}
/** Clamp to [1, sampleRate·4]. */
export function clampFpp(fpp: number, sampleRate: number): number {
  return fpp
}
export function frameToX(frame: number, scrollFrames: number, fpp: number): number {
  return (frame - scrollFrames) / fpp
}
export function xToFrame(x: number, scrollFrames: number, fpp: number): number {
  return scrollFrames + x * fpp
}
/** Zoom keeping the frame under `anchorX` fixed. Result clamped (fpp range, scroll ≥ 0). */
export function zoomAt(
  fpp: number,
  scrollFrames: number,
  factor: number,
  anchorX: number,
  sampleRate: number,
): { fpp: number; scrollFrames: number } {
  return { fpp, scrollFrames }
}
/** Fit `projectEnd·1.05` (min 1 s span) into `viewportW`; scroll 0. */
export function fitFpp(projectEnd: number, viewportW: number, sampleRate: number): number {
  return defaultFpp(sampleRate)
}
/** Zoom slider t∈[0,1] (1 = most zoomed in) ↔ fpp, log scale. */
export function sliderToFpp(t: number, sampleRate: number): number {
  return defaultFpp(sampleRate)
}
export function fppToSlider(fpp: number, sampleRate: number): number {
  return 0.5
}
/** Visible frame range `[start, end)`. */
export function visibleRange(scrollFrames: number, fpp: number, viewportW: number): [number, number] {
  return [scrollFrames, scrollFrames + viewportW * fpp]
}
/** Horizontal scroll extent in frames: max(projectEnd + 30 s, view end). */
export function scrollExtent(projectEnd: number, sampleRate: number, scrollFrames: number, fpp: number, viewportW: number): number {
  return projectEnd
}
