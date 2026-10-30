// Mixer dB maths: fader law (ui-spec §4.1), meter law (§4.2), level bands, parsing and display.
// Pure functions only (no DOM, no store). Every table point of the spec is pinned in dbScale.test.ts.

import { MAX_TRACK_GAIN_DB, MIN_GAIN_DB } from './types'

/** Engine silence / `linearToDb` floor. Values at or below this draw nothing. */
export const DB_FLOOR = -120

/** Piecewise-linear knot `[x, y]`; knots are ascending in `x`. */
type Knot = readonly [number, number]

/** Fader law §4.1, `[position, dB]`, ascending by position (spec lists it top-down). */
const FADER_KNOTS: readonly Knot[] = [
  [0, -96],
  [0.07, -60],
  [0.19, -40],
  [0.29, -30],
  [0.42, -20],
  [0.58, -10],
  [0.75, 0],
  [0.875, 6],
  [1, 12],
]
/** Same law as `[dB, position]`, ascending by dB, for the inverse mapping. */
const FADER_KNOTS_INV: readonly Knot[] = FADER_KNOTS.map(([pos, db]): Knot => [db, pos])

/** Positions at or below this are the bottom of the fader: −∞. */
const FADER_FLOOR_POS = 0.005
/** dB at or below this map to position 0 (−∞ end). */
const FADER_SILENCE_DB = -90

/** Meter law §4.2, `[dBFS, position]`, ascending by dB. Range −60…+3 dBFS. */
const METER_KNOTS: readonly Knot[] = [
  [-60, 0],
  [-40, 0.12],
  [-30, 0.24],
  [-20, 0.42],
  [-10, 0.64],
  [-6, 0.76],
  [0, 0.92],
  [3, 1],
]
const METER_KNOTS_INV: readonly Knot[] = METER_KNOTS.map(([db, pos]): Knot => [pos, db])

/** Segment colour bands by the segment's own dBFS (§4.2, tokens `--meter-*-db`). */
export const BAND_WARN_DB = -18
export const BAND_HOT_DB = -6
export const BAND_CLIP_DB = -1

/** Pitch of one LED segment: 3px lit + 1px gap (tokens `--meter-seg-h` / `--meter-seg-gap`). */
export const METER_SEG_PITCH_PX = 4

/** Fader scale labels, top to bottom (§4.1). */
export const FADER_SCALE_DB: readonly number[] = [12, 6, 0, -10, -20, -30, -40, -60, MIN_GAIN_DB]

/** Keyboard steps of the fader (§7). */
export const FADER_KEY_STEP_DB = 0.5
export const FADER_FINE_STEP_DB = 0.1
export const FADER_PAGE_STEP_DB = 3

export type LevelBand = 'ok' | 'warn' | 'hot' | 'clip'

export interface ScaleLabel {
  db: number
  text: string
  /** Vertical centre of the label, px from the top of the track. */
  y: number
  isZero: boolean
}

/** Piecewise-linear interpolation; clamps outside the knot range; NaN maps to the first knot. */
function piecewise(knots: readonly Knot[], x: number): number {
  const first = knots[0]
  const last = knots[knots.length - 1]
  if (Number.isNaN(x) || x <= first[0]) return first[1]
  if (x >= last[0]) return last[1]
  for (let i = 1; i < knots.length; i++) {
    const [x1, y1] = knots[i]
    if (x <= x1) {
      const [x0, y0] = knots[i - 1]
      return y0 + ((y1 - y0) * (x - x0)) / (x1 - x0)
    }
  }
  return last[1]
}

/** Clamp to the track-gain range −96…+12 dB. NaN becomes −∞. */
export function clampGainDb(db: number): number {
  if (Number.isNaN(db)) return MIN_GAIN_DB
  return Math.min(MAX_TRACK_GAIN_DB, Math.max(MIN_GAIN_DB, db))
}

/** Fader position 0 (bottom) … 1 (top) to dB. `pos ≤ 0.005` is −∞ (MIN_GAIN_DB). */
export function faderPosToDb(pos: number): number {
  if (!(pos > FADER_FLOOR_POS)) return MIN_GAIN_DB
  return piecewise(FADER_KNOTS, Math.min(1, pos))
}

/** dB to fader position. `db ≤ −90` (and NaN / −∞) is position 0. */
export function dbToFaderPos(db: number): number {
  if (!(db > FADER_SILENCE_DB)) return 0
  return piecewise(FADER_KNOTS_INV, db)
}

/** Meter level in dBFS to bar position 0 (bottom, −60) … 1 (top, +3). */
export function dbToMeterPos(db: number): number {
  return piecewise(METER_KNOTS, db)
}

/** Inverse of `dbToMeterPos`. */
export function meterPosToDb(pos: number): number {
  return piecewise(METER_KNOTS_INV, pos)
}

/** Linear amplitude to dBFS. `x ≤ 1e-6` (and NaN) is the −120 dB floor. */
export function linearToDb(x: number): number {
  return x > 1e-6 ? 20 * Math.log10(x) : DB_FLOOR
}

/** Colour band of a lit segment, judged by its own dBFS: ≤ −18 ok, ≤ −6 warn, ≤ −1 hot, else clip. */
export function levelBand(db: number): LevelBand {
  if (Number.isNaN(db) || db <= BAND_WARN_DB) return 'ok'
  if (db <= BAND_HOT_DB) return 'warn'
  if (db <= BAND_CLIP_DB) return 'hot'
  return 'clip'
}

/** Number of LED segments that fit in `heightPx` (floor(h / 4)); 0 for non-positive heights. */
export function meterSegmentCount(heightPx: number, pitchPx: number = METER_SEG_PITCH_PX): number {
  if (!Number.isFinite(heightPx) || heightPx <= 0) return 0
  return Math.floor(heightPx / pitchPx)
}

/**
 * How many segments from the bottom are lit for a bar position: `ceil(pos · count)`.
 * Position 0 lights nothing; position 1 lights every segment.
 */
export function litSegmentCount(pos: number, count: number): number {
  if (count <= 0 || !(pos > 0)) return 0
  // The epsilon keeps float noise (3.0000000000000004) from lighting an extra segment.
  return Math.min(count, Math.ceil(Math.min(1, pos) * count - 1e-9))
}

/** dBFS represented by segment `index` (0 = bottom), taken at the segment's centre. */
export function segmentDb(index: number, count: number): number {
  return meterPosToDb((index + 0.5) / Math.max(1, count))
}

/** Text of a fader scale label: `+12`, `0`, `−10`, `−∞`. */
export function scaleLabelText(db: number): string {
  if (db <= FADER_SILENCE_DB) return '−∞'
  if (db > 0) return `+${db}`
  return String(db).replace('-', '−')
}

/**
 * Lays out the fader scale for a track of `trackHeightPx` with a cap of `capHeightPx`.
 * Labels are visited top to bottom and a label closer than `minGapPx` to the previous shown one is omitted.
 */
export function faderScaleLayout(trackHeightPx: number, capHeightPx: number, minGapPx = 14): ScaleLabel[] {
  const travel = Math.max(0, trackHeightPx - capHeightPx)
  const labels: ScaleLabel[] = []
  let lastY = Number.NEGATIVE_INFINITY
  for (const db of FADER_SCALE_DB) {
    const y = capHeightPx / 2 + (1 - dbToFaderPos(db)) * travel
    if (y - lastY < minGapPx) continue
    labels.push({ db, text: scaleLabelText(db), y, isZero: db === 0 })
    lastY = y
  }
  return labels
}

/**
 * Parses user gain text: `-6`, `+3`, `-6.5db`, `−6` (U+2212), `-inf`, `-∞`.
 * Returns the raw dB (not clamped; use `clampGainDb`), or null when the text is not a number.
 */
export function parseDbInput(text: string): number | null {
  const s = text.trim().replace(/−/g, '-').replace(/db$/i, '').trim()
  if (/^-(inf(inity)?|∞)$/i.test(s)) return MIN_GAIN_DB
  if (!/^[+-]?(\d+\.?\d*|\.\d+)$/.test(s)) return null
  const value = Number(s)
  return Number.isFinite(value) ? value : null
}

/** Display text of a dB value: `−∞` at or below −90, otherwise one decimal with U+2212 minus, `+3.0`, `0.0`. */
export function formatDbReadout(db: number): string {
  if (!(db > FADER_SILENCE_DB)) return '−∞'
  const rounded = Math.round(db * 10) / 10
  if (rounded === 0) return '0.0'
  const sign = rounded < 0 ? '−' : '+'
  return sign + Math.abs(rounded).toFixed(1)
}

/**
 * Keyboard target of the fader (§7). Returns the new dB, or null for keys the fader does not handle.
 * Arrows ±0.5 dB (Shift ±0.1), PgUp/PgDn ±3, Home −∞, End +12, `0` resets to 0 dB.
 */
export function faderKeyDb(db: number, key: string, fine: boolean): number | null {
  const step = fine ? FADER_FINE_STEP_DB : FADER_KEY_STEP_DB
  switch (key) {
    case 'ArrowUp':
    case 'ArrowRight':
      return clampGainDb(db + step)
    case 'ArrowDown':
    case 'ArrowLeft':
      return clampGainDb(db - step)
    case 'PageUp':
      return clampGainDb(db + FADER_PAGE_STEP_DB)
    case 'PageDown':
      return clampGainDb(db - FADER_PAGE_STEP_DB)
    case 'Home':
      return MIN_GAIN_DB
    case 'End':
      return MAX_TRACK_GAIN_DB
    case '0':
      return 0
    default:
      return null
  }
}
