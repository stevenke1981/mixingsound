// Display formatting and parsing for time, dB, pan and file names (ui-spec §4.4). Pure, no DOM.

import { MIN_GAIN_DB } from './types'

const MINUS = '\u2212'
const ELLIPSIS = '\u2026'
const SILENT_DB = -90
const NEGATIVE_INFINITY_LABEL = `${MINUS}\u221e`
const POSITIVE_INFINITY_LABEL = '+\u221e'
const MS_PER_SECOND = 1000
const SECONDS_PER_MINUTE = 60
const SECONDS_PER_HOUR = 3600
const PAN_SCALE = 100
const PEAK_OK_DB = -1
const PEAK_CLIP_DB = 0

const SECONDS_TEXT = /^\d+(\.\d+)?$/
const INTEGER_TEXT = /^\d+$/
const NUMBER_TEXT = /^[+-]?(\d+(\.\d*)?|\.\d+)$/
const NEGATIVE_INFINITY_TEXT = /^-\s*(inf|infinity|\u221e)$/i
const DB_SUFFIX = /\s*db$/i

export type PeakTone = 'ok' | 'warn' | 'clip'

function finiteOr(value: number, fallback: number): number {
  return Number.isFinite(value) ? value : fallback
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, '0')
}

/** Thousands-separated integer, e.g. `1,234,567`. */
export function formatFrames(frames: number): string {
  const whole = Math.round(finiteOr(frames, 0))
  return String(whole).replace(/\B(?=(\d{3})+(?!\d))/g, ',')
}

export function framesToMs(frames: number, sampleRate: number): number {
  return sampleRate > 0 ? (finiteOr(frames, 0) * MS_PER_SECOND) / sampleRate : 0
}

export function msToFrames(ms: number, sampleRate: number): number {
  return Math.round((finiteOr(ms, 0) * sampleRate) / MS_PER_SECOND)
}

/** Sample rate as shown in the UI: `48`, `44.1`. */
export function formatKhz(sampleRate: number): string {
  return String(sampleRate / 1000)
}

/** `m:ss.mmm` (`00:12.345`); `h:mm:ss.mmm` from one hour on. Negative values clamp to zero. */
export function formatTime(frames: number, sampleRate: number): string {
  const totalMs = Math.max(0, Math.round(framesToMs(finiteOr(frames, 0), sampleRate)))
  const ms = totalMs % MS_PER_SECOND
  const totalSeconds = Math.floor(totalMs / MS_PER_SECOND)
  const minutes = Math.floor(totalSeconds / SECONDS_PER_MINUTE) % SECONDS_PER_MINUTE
  const seconds = totalSeconds % SECONDS_PER_MINUTE
  const clock = `${pad(minutes, 2)}:${pad(seconds, 2)}.${pad(ms, 3)}`
  const hours = Math.floor(totalSeconds / SECONDS_PER_HOUR)
  return hours > 0 ? `${hours}:${clock}` : clock
}

/**
 * Inverse of `formatTime` for typed input: `12.5`, `01:23.456`, `1:02:03.004`.
 * Returns frames, or `null` when the text is not a time.
 */
export function parseTime(text: string, sampleRate: number): number | null {
  if (!(sampleRate > 0)) return null
  const parts = text.trim().split(':')
  if (parts.length > 3) return null
  const [seconds, ...higher] = parts.reverse()
  if (seconds === undefined || !SECONDS_TEXT.test(seconds)) return null
  const secondsValue = Number(seconds)
  if (higher.length > 0 && secondsValue >= SECONDS_PER_MINUTE) return null
  let total = secondsValue
  let unit = 1
  for (const part of higher) {
    if (!INTEGER_TEXT.test(part)) return null
    unit *= SECONDS_PER_MINUTE
    total += Number(part) * unit
  }
  return Math.round(total * sampleRate)
}

/** dB readout: one decimal, U+2212 minus, `+` above zero; `−∞` at or below −90 dB (and for NaN). */
export function formatDb(db: number): string {
  if (Number.isNaN(db) || db <= SILENT_DB) return NEGATIVE_INFINITY_LABEL
  if (db === Number.POSITIVE_INFINITY) return POSITIVE_INFINITY_LABEL
  const rounded = Math.round(db * 10) / 10
  const body = Math.abs(rounded).toFixed(1)
  if (rounded > 0) return `+${body}`
  if (rounded < 0) return `${MINUS}${body}`
  return body
}

/** Accepts `-6`, `−6.5`, `+3`, `-6.5db`, `-inf`, `−∞`. Returns dB, or `null` when not a number. */
export function parseDb(text: string): number | null {
  const normalized = text.trim().replace(/\u2212/g, '-').replace(DB_SUFFIX, '').trim()
  if (NEGATIVE_INFINITY_TEXT.test(normalized)) return MIN_GAIN_DB
  if (!NUMBER_TEXT.test(normalized)) return null
  const value = Number(normalized)
  return Number.isFinite(value) ? value : null
}

/** Pan readout: `C`, `L 32`, `R 18` (round(|p|·100)). */
export function formatPan(pan: number): string {
  const clamped = Math.min(1, Math.max(-1, finiteOr(pan, 0)))
  const amount = Math.round(Math.abs(clamped) * PAN_SCALE)
  if (amount === 0) return 'C'
  return clamped < 0 ? `L ${amount}` : `R ${amount}`
}

/** Colour band of a peak readout in the export report (ui-spec §2.10). */
export function peakTone(db: number): PeakTone {
  if (db > PEAK_CLIP_DB) return 'clip'
  if (db > PEAK_OK_DB) return 'warn'
  return 'ok'
}

/** File name without directory or extension: `C:/a/My Mix.msproj` → `My Mix`. */
export function fileStem(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? ''
  return name.replace(/\.[^.]*$/, '')
}

/** Shortens long text by replacing its middle with an ellipsis, keeping both ends. */
export function middleTruncate(text: string, max: number): string {
  if (max < 3 || text.length <= max) return text
  const keep = max - 1
  const head = Math.ceil(keep / 2)
  const tail = Math.floor(keep / 2)
  return `${text.slice(0, head)}${ELLIPSIS}${text.slice(text.length - tail)}`
}
