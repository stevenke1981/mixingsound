// Timeline canvas painting helpers and pure text formatters (ui-spec §2.3, §2.5, §3.1, §5.3).
// Nothing here reads the store: callers pass geometry, look and a waveform sampler, so the
// functions stay testable and LaneCanvas owns all state.
import { LANE_H } from '../../lib/geometry'
import { CLIP_INSET_Y, HEADER_STRIP_H } from '../../lib/hitTest'

const MINUS = '−'
const INFINITY_SIGN = '∞'
const ELLIPSIS = '…'
const DB_FLOOR = -90
const WAVE_TINT = 0.35
const SELECTED_TINT = 0.12
const BODY_TINT_ALPHA = 0.22
const WAVE_CENTRE_ALPHA = 0.12
const WAVE_MARGIN_PX = 3
const HATCH_STEP_PX = 6
const HATCH_ALPHA = 0.35
const MISSING_TINT_ALPHA = 0.12
const FADE_SHADE_ALPHA = 0.4
const FADE_LINE_ALPHA = 0.6
const HANDLE_PX = 8
const MIN_LABEL_W = 40
const LABEL_PAD = 6
const BUBBLE_H = 18
const BUBBLE_PAD = 6
const BUBBLE_ALPHA = 0.95
const GUIDE_DASH: number[] = [4, 3]
const GAIN_BADGE_ZERO = '0.0'
const FALLBACK_COLOR = 'transparent'
const FALLBACK_FONT = 'sans-serif'
const FALLBACK_FS_2XS = 10
const FALLBACK_FS_XS = 11
const FALLBACK_RADIUS = 3

/** Resolved token values for canvas painting. Read once per component mount. */
export interface LanePalette {
  bg0: string
  bg1: string
  bg2: string
  laneA: string
  laneB: string
  line: string
  text: string
  textMuted: string
  textDisabled: string
  accent: string
  accentHi: string
  accentWash: string
  select: string
  well: string
  clipInk: string
  danger: string
  meterClip: string
  fontLabel: string
  fs2xs: number
  fsXs: number
  radius: number
}

/** Pixel rectangle of a clip in canvas CSS px. `y` is the clip top (header strip). */
export interface ClipGeo {
  x: number
  y: number
  w: number
  h: number
  /** Canvas CSS width; waveform columns are clamped to it. */
  viewW: number
  fadeInPx: number
  fadeOutPx: number
}

export interface ClipLook {
  /** Header text (source name). Ignored when `missing`. */
  name: string
  /** Track colour as CSS colour (engine emits `#rrggbb`). */
  color: string
  gainDb: number
  missing: boolean
  selected: boolean
  hovered: boolean
  showHandles: boolean
  ghost: boolean
  /** Track-level opacity: muted 0.4, solo-excluded 0.55, dragged original 0.35, else 1. */
  alpha: number
}

/** Linear `[min, max]` for one canvas column, or null when no tile is loaded yet. */
export type WaveSample = (col: number) => [number, number] | null

/** Preview state of a clip gesture. Only the ghost is drawn during the gesture (ui-spec §3.1). */
export interface GhostState {
  start: number
  offset: number
  length: number
  fadeIn: number
  fadeOut: number
  gainDb: number
  trackIndex: number
}

export type ClipGestureKind = 'move' | 'trimL' | 'trimR' | 'fadeIn' | 'fadeOut' | 'gain'

function token(style: CSSStyleDeclaration, name: string, fallback: string): string {
  const value = style.getPropertyValue(name).trim()
  return value === '' ? fallback : value
}

function tokenPx(style: CSSStyleDeclaration, name: string, fallback: number): number {
  const value = Number.parseFloat(token(style, name, ''))
  return Number.isFinite(value) ? value : fallback
}

/** Reads the token values used by canvas code. Missing tokens fall back to inert values. */
export function readLanePalette(style: CSSStyleDeclaration): LanePalette {
  const color = (name: string): string => token(style, name, FALLBACK_COLOR)
  return {
    bg0: color('--bg-0'),
    bg1: color('--bg-1'),
    bg2: color('--bg-2'),
    laneA: color('--lane-a'),
    laneB: color('--lane-b'),
    line: color('--line'),
    text: color('--text'),
    textMuted: color('--text-muted'),
    textDisabled: color('--text-disabled'),
    accent: color('--accent'),
    accentHi: color('--accent-hi'),
    accentWash: color('--accent-wash'),
    select: color('--select'),
    well: color('--well'),
    clipInk: color('--clip-ink'),
    danger: color('--danger'),
    meterClip: color('--meter-clip'),
    fontLabel: token(style, '--font-label', FALLBACK_FONT),
    fs2xs: tokenPx(style, '--fs-2xs', FALLBACK_FS_2XS),
    fsXs: tokenPx(style, '--fs-xs', FALLBACK_FS_XS),
    radius: tokenPx(style, '--r-sm', FALLBACK_RADIUS),
  }
}

// ---- Pure formatters ---------------------------------------------------------------------

/** `−6.0`, `+3.0`, `0.0`, `−∞` (≤ −90 dB). Private to the timeline; lib/format.ts is not used. */
export function formatDbShort(db: number): string {
  if (Number.isNaN(db) || db <= DB_FLOOR) return `${MINUS}${INFINITY_SIGN}`
  const rounded = Math.round(db * 10) / 10
  if (rounded === 0) return GAIN_BADGE_ZERO
  return rounded > 0 ? `+${rounded.toFixed(1)}` : `${MINUS}${Math.abs(rounded).toFixed(1)}`
}

/** `−6.0 dB`. */
export function formatDbUnit(db: number): string {
  return `${formatDbShort(db)} dB`
}

/** Gain badge for the clip header; null when the gain is 0.0 dB (badge hidden). */
export function formatGainBadge(db: number): string | null {
  return formatDbShort(db) === GAIN_BADGE_ZERO ? null : formatDbUnit(db)
}

/** `C`, `L 32`, `R 18` (round(|pan|·100)). */
export function formatPan(pan: number): string {
  const amount = Math.round(Math.abs(pan) * 100)
  if (amount === 0) return 'C'
  return pan < 0 ? `L ${amount}` : `R ${amount}`
}

/** Track header summary: `−6.0 dB · L 32`. */
export function formatTrackSummary(gainDb: number, pan: number): string {
  return `${formatDbUnit(gainDb)} · ${formatPan(pan)}`
}

function pad(value: number, width: number): string {
  return String(value).padStart(width, '0')
}

/** `mm:ss.mmm`, or `h:mm:ss.mmm` from one hour. Matches the ruler and the clip bubbles. */
export function formatClock(frames: number, sampleRate: number): string {
  if (sampleRate <= 0) return '00:00.000'
  const totalMs = Math.max(0, Math.round((frames / sampleRate) * 1000))
  const millis = totalMs % 1000
  const totalSec = Math.floor(totalMs / 1000)
  const minutesAndSeconds = `${pad(Math.floor(totalSec / 60) % 60, 2)}:${pad(totalSec % 60, 2)}.${pad(millis, 3)}`
  const hours = Math.floor(totalSec / 3600)
  return hours > 0 ? `${hours}:${minutesAndSeconds}` : minutesAndSeconds
}

/** `+0.250 s` / `−0.250 s` for a frame delta. */
export function formatSignedSeconds(frames: number, sampleRate: number): string {
  const seconds = sampleRate > 0 ? frames / sampleRate : 0
  const sign = seconds < 0 ? MINUS : '+'
  return `${sign}${Math.abs(seconds).toFixed(3)} s`
}

/** `#rrggbb` mixed toward white by `t` (0..1) → `rgb(...)`. Null when the input is not 6-digit hex. */
export function mixHex(hex: string, t: number): string | null {
  const match = /^#([0-9a-f]{6})$/i.exec(hex.trim())
  if (!match) return null
  const value = Number.parseInt(match[1] ?? '0', 16)
  const mix = (channel: number): number => Math.round(channel + (255 - channel) * t)
  const r = mix((value >> 16) & 0xff)
  const g = mix((value >> 8) & 0xff)
  const b = mix(value & 0xff)
  return `rgb(${r} ${g} ${b})`
}

/** Shortens `text` with an ellipsis until `measure(text) <= maxW`. Empty when nothing fits. */
export function ellipsize(measure: (text: string) => number, text: string, maxW: number): string {
  if (maxW <= 0) return ''
  if (measure(text) <= maxW) return text
  for (let length = text.length - 1; length > 0; length--) {
    const candidate = `${text.slice(0, length)}${ELLIPSIS}`
    if (measure(candidate) <= maxW) return candidate
  }
  return ''
}

/** Live readout shown next to the pointer during a gesture (ui-spec §3.1). */
export function gestureBubble(
  kind: ClipGestureKind,
  ghost: GhostState,
  originStart: number,
  sampleRate: number,
): string {
  switch (kind) {
    case 'move':
      return `Start ${formatClock(ghost.start, sampleRate)} (${formatSignedSeconds(ghost.start - originStart, sampleRate)})`
    case 'trimL':
    case 'trimR':
      return `Start ${formatClock(ghost.start, sampleRate)} · Length ${formatClock(ghost.length, sampleRate)}`
    case 'fadeIn':
      return `Fade in ${framesToMs(ghost.fadeIn, sampleRate)} ms`
    case 'fadeOut':
      return `Fade out ${framesToMs(ghost.fadeOut, sampleRate)} ms`
    case 'gain':
      return formatDbUnit(ghost.gainDb)
  }
}

function framesToMs(frames: number, sampleRate: number): number {
  return sampleRate > 0 ? Math.round((frames / sampleRate) * 1000) : 0
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value))
}

export function dbToLinear(db: number): number {
  return 10 ** (db / 20)
}

// ---- Painting ----------------------------------------------------------------------------

/** Lane background and bottom hairline. `y` is the lane top in canvas px. */
export function drawLaneBackground(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  y: number,
  index: number,
  w: number,
): void {
  ctx.fillStyle = index % 2 === 0 ? pal.laneA : pal.laneB
  ctx.fillRect(0, y, w, LANE_H)
  ctx.fillStyle = pal.line
  ctx.fillRect(0, y + LANE_H - 1, w, 1)
}

/** Loop region tint across all lanes (under the clips). Dimmer when the loop is disabled. */
export function drawLoopTint(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  x0: number,
  x1: number,
  h: number,
  enabled: boolean,
): void {
  ctx.save()
  ctx.globalAlpha = enabled ? 0.5 : 0.25
  ctx.fillStyle = pal.accentWash
  ctx.fillRect(x0, 0, x1 - x0, h)
  if (enabled) {
    ctx.globalAlpha = 1
    ctx.fillStyle = pal.accent
    ctx.fillRect(x0, 0, 1, h)
    ctx.fillRect(x1 - 1, 0, 1, h)
  }
  ctx.restore()
}

/** Dashed vertical snap guide (ui-spec §2.5). */
export function drawSnapGuide(ctx: CanvasRenderingContext2D, pal: LanePalette, x: number, h: number): void {
  ctx.save()
  ctx.strokeStyle = pal.accentHi
  ctx.lineWidth = 1
  ctx.setLineDash(GUIDE_DASH)
  ctx.beginPath()
  ctx.moveTo(Math.round(x) + 0.5, 0)
  ctx.lineTo(Math.round(x) + 0.5, h)
  ctx.stroke()
  ctx.restore()
}

/** Small label bubble near the pointer while a gesture runs. */
export function drawBubble(ctx: CanvasRenderingContext2D, pal: LanePalette, x: number, y: number, text: string): void {
  ctx.save()
  ctx.font = `600 ${pal.fsXs}px ${pal.fontLabel}`
  const w = ctx.measureText(text).width + BUBBLE_PAD * 2
  ctx.globalAlpha = BUBBLE_ALPHA
  ctx.fillStyle = pal.bg2
  ctx.beginPath()
  ctx.roundRect(x, y, w, BUBBLE_H, pal.radius)
  ctx.fill()
  ctx.globalAlpha = 1
  ctx.fillStyle = pal.text
  ctx.textBaseline = 'middle'
  ctx.fillText(text, x + BUBBLE_PAD, y + BUBBLE_H / 2)
  ctx.restore()
}

/** Paints one clip: body tint, waveform, fades, header strip, outline and fade handles. */
export function drawClip(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  geo: ClipGeo,
  look: ClipLook,
  sample: WaveSample,
): void {
  if (geo.w <= 0 || geo.h <= HEADER_STRIP_H) return
  ctx.save()
  ctx.beginPath()
  ctx.roundRect(geo.x, geo.y, geo.w, geo.h, pal.radius)
  ctx.clip()
  paintBody(ctx, pal, geo, look, sample)
  paintHeader(ctx, pal, geo, look)
  ctx.restore()
  paintOutline(ctx, pal, geo, look)
  if (look.showHandles && !look.ghost) paintHandles(ctx, pal, geo)
}

function paintBody(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  geo: ClipGeo,
  look: ClipLook,
  sample: WaveSample,
): void {
  const top = geo.y + HEADER_STRIP_H
  const bodyH = geo.h - HEADER_STRIP_H
  ctx.globalAlpha = look.alpha * BODY_TINT_ALPHA
  ctx.fillStyle = look.color
  ctx.fillRect(geo.x, top, geo.w, bodyH)
  if (look.missing) {
    paintMissing(ctx, pal, geo, look, top, bodyH)
  } else {
    paintWave(ctx, pal, geo, look, sample)
  }
  paintFades(ctx, pal, geo, look)
}

function paintMissing(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  geo: ClipGeo,
  look: ClipLook,
  top: number,
  bodyH: number,
): void {
  ctx.globalAlpha = look.alpha * MISSING_TINT_ALPHA
  ctx.fillStyle = pal.danger
  ctx.fillRect(geo.x, top, geo.w, bodyH)
  ctx.globalAlpha = look.alpha * HATCH_ALPHA
  ctx.strokeStyle = pal.danger
  ctx.lineWidth = 1
  ctx.beginPath()
  for (let k = -bodyH; k < geo.w; k += HATCH_STEP_PX) {
    ctx.moveTo(geo.x + k, top + bodyH)
    ctx.lineTo(geo.x + k + bodyH, top)
  }
  ctx.stroke()
}

function paintWave(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  geo: ClipGeo,
  look: ClipLook,
  sample: WaveSample,
): void {
  const x0 = Math.max(0, Math.floor(geo.x))
  const x1 = Math.min(geo.viewW, Math.ceil(geo.x + geo.w))
  if (x1 <= x0) return
  const top = geo.y + HEADER_STRIP_H
  const bodyH = geo.h - HEADER_STRIP_H
  const mid = top + bodyH / 2
  const half = Math.max(1, bodyH / 2 - WAVE_MARGIN_PX)
  const gain = dbToLinear(look.gainDb)
  const waveColor = mixHex(look.color, WAVE_TINT) ?? look.color

  ctx.globalAlpha = look.alpha * WAVE_CENTRE_ALPHA
  ctx.fillStyle = pal.text
  ctx.fillRect(x0, Math.round(mid), x1 - x0, 1)

  ctx.globalAlpha = look.alpha
  ctx.fillStyle = waveColor
  for (let col = x0; col < x1; col++) {
    const minMax = sample(col)
    if (minMax) paintColumn(ctx, pal, col, minMax, gain, mid, half, waveColor)
  }
}

function paintColumn(
  ctx: CanvasRenderingContext2D,
  pal: LanePalette,
  col: number,
  [lo, hi]: [number, number],
  gain: number,
  mid: number,
  half: number,
  waveColor: string,
): void {
  const hiLin = hi * gain
  const loLin = lo * gain
  const yTop = mid - clamp(hiLin, -1, 1) * half
  const yBot = mid - clamp(loLin, -1, 1) * half
  ctx.fillRect(col, yTop, 1, Math.max(1, yBot - yTop))
  if (hiLin > 1 || loLin < -1) {
    ctx.fillStyle = pal.meterClip
    if (hiLin > 1) ctx.fillRect(col, mid - half, 1, 1)
    if (loLin < -1) ctx.fillRect(col, mid + half - 1, 1, 1)
    ctx.fillStyle = waveColor
  }
}

/** Darkened fade regions (above the ramp) plus the ramp lines. */
function paintFades(ctx: CanvasRenderingContext2D, pal: LanePalette, geo: ClipGeo, look: ClipLook): void {
  const top = geo.y + HEADER_STRIP_H
  const bottom = geo.y + geo.h
  const fadeIn = clamp(geo.fadeInPx, 0, geo.w)
  const fadeOut = clamp(geo.fadeOutPx, 0, geo.w)
  const right = geo.x + geo.w
  ctx.globalAlpha = look.alpha * FADE_SHADE_ALPHA
  ctx.fillStyle = pal.well
  ctx.beginPath()
  if (fadeIn > 0) {
    ctx.moveTo(geo.x, top)
    ctx.lineTo(geo.x + fadeIn, top)
    ctx.lineTo(geo.x, bottom)
    ctx.closePath()
  }
  if (fadeOut > 0) {
    ctx.moveTo(right - fadeOut, top)
    ctx.lineTo(right, top)
    ctx.lineTo(right, bottom)
    ctx.closePath()
  }
  ctx.fill()
  ctx.globalAlpha = look.alpha * FADE_LINE_ALPHA
  ctx.strokeStyle = pal.select
  ctx.lineWidth = 1
  ctx.beginPath()
  if (fadeIn > 0) {
    ctx.moveTo(geo.x, bottom)
    ctx.lineTo(geo.x + fadeIn, top)
  }
  if (fadeOut > 0) {
    ctx.moveTo(right - fadeOut, top)
    ctx.lineTo(right, bottom)
  }
  ctx.stroke()
}

function paintHeader(ctx: CanvasRenderingContext2D, pal: LanePalette, geo: ClipGeo, look: ClipLook): void {
  ctx.globalAlpha = look.alpha
  ctx.fillStyle = look.selected ? (mixHex(look.color, SELECTED_TINT) ?? look.color) : look.color
  ctx.fillRect(geo.x, geo.y, geo.w, HEADER_STRIP_H)
  if (geo.w < MIN_LABEL_W) return
  const mid = geo.y + HEADER_STRIP_H / 2
  ctx.font = `600 ${pal.fsXs}px ${pal.fontLabel}`
  ctx.fillStyle = pal.clipInk
  ctx.textBaseline = 'middle'
  ctx.textAlign = 'left'
  const badge = look.missing ? null : formatGainBadge(look.gainDb)
  const badgeW = badge ? ctx.measureText(badge).width + LABEL_PAD : 0
  const label = look.missing ? 'MISSING' : look.name
  const maxW = geo.w - LABEL_PAD * 2 - badgeW
  ctx.fillText(ellipsize((text) => ctx.measureText(text).width, label, maxW), geo.x + LABEL_PAD, mid)
  if (badge) {
    ctx.textAlign = 'right'
    ctx.fillText(badge, geo.x + geo.w - LABEL_PAD, mid)
    ctx.textAlign = 'left'
  }
}

function paintOutline(ctx: CanvasRenderingContext2D, pal: LanePalette, geo: ClipGeo, look: ClipLook): void {
  ctx.save()
  ctx.beginPath()
  if (look.selected || look.ghost) {
    ctx.lineWidth = 2
    ctx.strokeStyle = pal.select
    ctx.globalAlpha = look.ghost ? 1 : look.alpha
    ctx.roundRect(geo.x + 1, geo.y + 1, geo.w - 2, geo.h - 2, pal.radius)
  } else {
    ctx.lineWidth = 1
    ctx.strokeStyle = pal.text
    ctx.globalAlpha = look.alpha * (look.hovered ? 0.25 : 0.08)
    ctx.roundRect(geo.x + 0.5, geo.y + 0.5, geo.w - 1, geo.h - 1, pal.radius)
  }
  ctx.stroke()
  ctx.restore()
}

/** Fade handles: 8 px squares on the fade boundaries at the top of the body. */
function paintHandles(ctx: CanvasRenderingContext2D, pal: LanePalette, geo: ClipGeo): void {
  const top = geo.y + HEADER_STRIP_H
  const fadeIn = clamp(geo.fadeInPx, 0, geo.w)
  const fadeOut = clamp(geo.fadeOutPx, 0, geo.w)
  ctx.save()
  ctx.globalAlpha = 1
  ctx.fillStyle = pal.select
  ctx.fillRect(geo.x + fadeIn - HANDLE_PX / 2, top, HANDLE_PX, HANDLE_PX)
  ctx.fillRect(geo.x + geo.w - fadeOut - HANDLE_PX / 2, top, HANDLE_PX, HANDLE_PX)
  ctx.restore()
}
