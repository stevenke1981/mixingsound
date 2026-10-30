// Timeline geometry (ui-spec §5.1). Pure: plain numbers in, plain numbers out.

export const LANE_H = 72
export const RULER_H = 28
export const HEADER_W = 208
/** Zoom step per Ctrl+wheel notch / zoom button. */
export const ZOOM_STEP = 1.25

/** Finest zoom: one frame per pixel. */
const FPP_MIN = 1
/** Coarsest zoom: four seconds of audio per pixel is past useful; spec caps at sampleRate·4. */
const FPP_MAX_SECONDS = 4
/** Minimum fit span so a near-empty project still shows a second. */
const FIT_MIN_SECONDS = 1
/** Empty runway past the last clip in the horizontal scroll extent. */
const SCROLL_RUNWAY_SECONDS = 30
/** Fit adds 5 % headroom to the project end. */
const FIT_HEADROOM = 1.05

function fppMax(sampleRate: number): number {
  return Math.max(FPP_MIN, sampleRate * FPP_MAX_SECONDS)
}

export function defaultFpp(sampleRate: number): number {
  return sampleRate / 100
}

/** Clamp to [1, sampleRate·4]. Non-finite input falls back to the default zoom. */
export function clampFpp(fpp: number, sampleRate: number): number {
  const value = Number.isFinite(fpp) ? fpp : defaultFpp(sampleRate)
  return Math.min(fppMax(sampleRate), Math.max(FPP_MIN, value))
}

export function frameToX(frame: number, scrollFrames: number, fpp: number): number {
  return (frame - scrollFrames) / fpp
}

export function xToFrame(x: number, scrollFrames: number, fpp: number): number {
  return scrollFrames + x * fpp
}

/**
 * Zoom keeping the frame under `anchorX` fixed. Result clamped (fpp range, scroll ≥ 0).
 * `factor` multiplies fpp: > 1 zooms out, < 1 zooms in (callers pass ZOOM_STEP or 1/ZOOM_STEP).
 * When the scroll clamp at 0 engages, the anchor frame moves to keep the view valid.
 */
export function zoomAt(
  fpp: number,
  scrollFrames: number,
  factor: number,
  anchorX: number,
  sampleRate: number,
): { fpp: number; scrollFrames: number } {
  const nextFpp = clampFpp(fpp * factor, sampleRate)
  const anchorFrame = xToFrame(anchorX, scrollFrames, fpp)
  const nextScroll = anchorFrame - anchorX * nextFpp
  return { fpp: nextFpp, scrollFrames: Math.max(0, nextScroll) }
}

/** Fit `projectEnd·1.05` (min 1 s span) into `viewportW`; scroll 0. */
export function fitFpp(projectEnd: number, viewportW: number, sampleRate: number): number {
  if (!(viewportW > 0)) return clampFpp(defaultFpp(sampleRate), sampleRate)
  const span = Math.max(projectEnd * FIT_HEADROOM, sampleRate * FIT_MIN_SECONDS)
  return clampFpp(span / viewportW, sampleRate)
}

/** Log scale over [1, sampleRate·4]; t = 1 is most zoomed in, t = 0 most zoomed out. `t` is clamped to [0, 1]. */
export function sliderToFpp(t: number, sampleRate: number): number {
  const position = Number.isFinite(t) ? Math.min(1, Math.max(0, t)) : 0.5
  const logMax = Math.log(fppMax(sampleRate))
  return clampFpp(Math.exp((1 - position) * logMax), sampleRate)
}

/** Inverse of `sliderToFpp`. Degenerate ranges (max ≤ 1) report the most-zoomed-in position. */
export function fppToSlider(fpp: number, sampleRate: number): number {
  const logMax = Math.log(fppMax(sampleRate))
  if (!(logMax > 0)) return 1
  const logFpp = Math.log(clampFpp(fpp, sampleRate))
  return Math.min(1, Math.max(0, 1 - logFpp / logMax))
}

/** Visible frame range `[start, end)`. */
export function visibleRange(scrollFrames: number, fpp: number, viewportW: number): [number, number] {
  return [scrollFrames, scrollFrames + viewportW * fpp]
}

/** Horizontal scroll extent in frames: max(projectEnd + 30 s, view end). */
export function scrollExtent(projectEnd: number, sampleRate: number, scrollFrames: number, fpp: number, viewportW: number): number {
  const viewEnd = scrollFrames + viewportW * fpp
  return Math.max(projectEnd + SCROLL_RUNWAY_SECONDS * sampleRate, viewEnd)
}
