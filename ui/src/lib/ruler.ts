// Ruler ticks (ui-spec §2.4). Pure.

export interface Tick {
  frame: number
  major: boolean
  /** Label for major ticks only. */
  label: string | null
}

/** Minimum pixel distance between two major labels. */
const MIN_LABEL_PX = 70
/** Hard cap on ticks per call so extreme zoom can never spin the loop. */
const MAX_TICKS = 2000
/** Decade range searched for the step; covers any zoom the geometry clamp allows. */
const MIN_EXPONENT = -6
const MAX_EXPONENT = 6
const STEP_MANTISSAS = [1, 2, 5] as const
/** Minor divisions per major: 5 for 1×/2× steps, 2 for 5× steps (minor = step/2). */
const MINOR_RATIO_FIVE = 5
const MINOR_RATIO_TWO = 2

interface TickPlan {
  majorSec: number
  minorSec: number
  /** Number of minor intervals per major interval. */
  ratio: number
}

/** Step from [1,2,5]×10^n seconds with labels ≥ 70 px apart; minor = step/5 (step/2 when step is 5×). */
export function tickStep(fpp: number, sampleRate: number): { majorSec: number; minorSec: number } {
  const { majorSec, minorSec } = planTicks(fpp, sampleRate)
  return { majorSec, minorSec }
}

export function ticks(scrollFrames: number, fpp: number, viewportW: number, sampleRate: number): Tick[] {
  if (!(fpp > 0) || !(sampleRate > 0) || !(viewportW > 0) || !Number.isFinite(scrollFrames)) return []

  const plan = planTicks(fpp, sampleRate)
  const minorFrames = plan.minorSec * sampleRate
  const startFrame = Math.max(0, scrollFrames)
  const endFrame = scrollFrames + viewportW * fpp
  const firstIndex = Math.ceil(startFrame / minorFrames)
  const lastIndex = Math.min(Math.floor(endFrame / minorFrames), firstIndex + MAX_TICKS - 1)

  const result: Tick[] = []
  for (let index = firstIndex; index <= lastIndex; index++) {
    const major = index % plan.ratio === 0
    result.push({
      frame: index * minorFrames,
      major,
      label: major ? formatRulerLabel((index / plan.ratio) * plan.majorSec, plan.majorSec) : null,
    })
  }
  return result
}

function planTicks(fpp: number, sampleRate: number): TickPlan {
  const pxPerSecond = sampleRate / fpp
  if (!(pxPerSecond > 0) || !Number.isFinite(pxPerSecond)) {
    return { majorSec: 1, minorSec: 0.2, ratio: MINOR_RATIO_FIVE }
  }
  for (let exponent = MIN_EXPONENT; exponent <= MAX_EXPONENT; exponent++) {
    for (const mantissa of STEP_MANTISSAS) {
      const majorSec = mantissa * 10 ** exponent
      if (majorSec * pxPerSecond >= MIN_LABEL_PX) return minorPlan(majorSec, mantissa)
    }
  }
  return minorPlan(5 * 10 ** MAX_EXPONENT, 5)
}

function minorPlan(majorSec: number, mantissa: number): TickPlan {
  if (mantissa === 5) return { majorSec, minorSec: majorSec / 2, ratio: MINOR_RATIO_TWO }
  return { majorSec, minorSec: majorSec / 5, ratio: MINOR_RATIO_FIVE }
}

/** `m:ss` for steps ≥ 1 s, `m:ss.mmm` below. Milliseconds are rounded once, in integers. */
function formatRulerLabel(seconds: number, stepSec: number): string {
  const totalMs = Math.round(seconds * 1000)
  const minutes = Math.floor(totalMs / 60000)
  const secondsPart = Math.floor((totalMs % 60000) / 1000)
  const ms = totalMs % 1000
  const base = `${minutes}:${String(secondsPart).padStart(2, '0')}`
  if (stepSec >= 1) return base
  return `${base}.${String(ms).padStart(3, '0')}`
}
