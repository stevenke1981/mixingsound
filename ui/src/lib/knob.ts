// Pan knob maths (ui-spec §2.6, §3.2, §7): angle mapping, drag / wheel / keyboard steps. Pure.
// Pan is −1 (hard left) … +1 (hard right). ARIA reports the same value as −100 … +100.

export const PAN_MIN = -1
export const PAN_MAX = 1
/** 270° sweep, centred on 12 o'clock. */
export const KNOB_MIN_DEG = -135
export const KNOB_MAX_DEG = 135

/** Vertical drag that spans the full −1…+1 range. */
export const DRAG_PX_FULL_RANGE = 150
export const FINE_FACTOR = 0.1

export const WHEEL_STEP = 0.02
export const WHEEL_FINE_STEP = 0.005
export const KEY_STEP = 0.02
export const KEY_FINE_STEP = 0.005
export const KEY_PAGE_STEP = 0.1

function clampTo(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

/** Clamps to −1…+1. NaN is centre. */
export function clampPan(pan: number): number {
  if (Number.isNaN(pan)) return 0
  return clampTo(pan, PAN_MIN, PAN_MAX)
}

/** Pan to the indicator angle in degrees: −1 → −135°, 0 → 0°, +1 → +135°. */
export function panToAngle(pan: number): number {
  const t = (clampPan(pan) - PAN_MIN) / (PAN_MAX - PAN_MIN)
  return KNOB_MIN_DEG + t * (KNOB_MAX_DEG - KNOB_MIN_DEG)
}

/** Inverse of `panToAngle`; angles outside the sweep clamp to the hard ends. */
export function angleToPan(deg: number): number {
  if (Number.isNaN(deg)) return 0
  const t = (clampTo(deg, KNOB_MIN_DEG, KNOB_MAX_DEG) - KNOB_MIN_DEG) / (KNOB_MAX_DEG - KNOB_MIN_DEG)
  return PAN_MIN + t * (PAN_MAX - PAN_MIN)
}

/**
 * Incremental vertical drag. `dyPx` is the pointer movement since the last event, positive = down,
 * which lowers the value. Shift (`fine`) scales the movement by 0.1, so toggling Shift mid-drag is smooth.
 */
export function dragStep(pan: number, dyPx: number, fine: boolean): number {
  if (!Number.isFinite(dyPx)) return clampPan(pan)
  const perPx = ((PAN_MAX - PAN_MIN) / DRAG_PX_FULL_RANGE) * (fine ? FINE_FACTOR : 1)
  return clampPan(clampPan(pan) - dyPx * perPx)
}

/** One wheel notch per event: up (negative deltaY) raises the value, down lowers it. */
export function wheelStep(pan: number, deltaY: number, fine: boolean): number {
  if (!Number.isFinite(deltaY) || deltaY === 0) return clampPan(pan)
  const step = fine ? WHEEL_FINE_STEP : WHEEL_STEP
  return clampPan(clampPan(pan) + (deltaY < 0 ? step : -step))
}

/**
 * Keyboard target of the knob (§7). Returns the new pan, or null for keys the knob does not handle.
 * ←/↓ and →/↑ move ±0.02 (Shift ±0.005), PgUp/PgDn ±0.1, Home/End hard left/right, `0` centre.
 */
export function keyStep(pan: number, key: string, fine: boolean): number | null {
  const step = fine ? KEY_FINE_STEP : KEY_STEP
  const current = clampPan(pan)
  switch (key) {
    case 'ArrowRight':
    case 'ArrowUp':
      return clampPan(current + step)
    case 'ArrowLeft':
    case 'ArrowDown':
      return clampPan(current - step)
    case 'PageUp':
      return clampPan(current + KEY_PAGE_STEP)
    case 'PageDown':
      return clampPan(current - KEY_PAGE_STEP)
    case 'Home':
      return PAN_MIN
    case 'End':
      return PAN_MAX
    case '0':
      return 0
    default:
      return null
  }
}

/** ARIA value: −100 … +100, integer, never −0. */
export function panAriaValue(pan: number): number {
  return Math.round(clampPan(pan) * 100) || 0
}

/** Short readout for tooltips: `C`, `L 32`, `R 18`. */
export function panShortLabel(pan: number): string {
  const n = panAriaValue(pan)
  if (n === 0) return 'C'
  return n < 0 ? `L ${-n}` : `R ${n}`
}

/** Spoken value for `aria-valuetext`: `Center`, `Left 32`, `Right 18`. */
export function panSpokenLabel(pan: number): string {
  const n = panAriaValue(pan)
  if (n === 0) return 'Center'
  return n < 0 ? `Left ${-n}` : `Right ${n}`
}
