import { describe, expect, it } from 'vitest'
import { DB_FLOOR } from './dbScale'
import {
  CLIP_LINEAR,
  HOLD_SECONDS,
  PEAK_FALL_DB_PER_S,
  RMS_FALL_DB_PER_S,
  initialBallistics,
  stepBallistics,
  type BallisticsInput,
  type BallisticsState,
} from './ballistics'

const SILENCE: BallisticsInput = { peakDb: DB_FLOOR, rmsDb: DB_FLOOR, peakLinear: 0 }

function loud(peakDb: number, rmsDb = peakDb - 6): BallisticsInput {
  return { peakDb, rmsDb, peakLinear: 10 ** (peakDb / 20) }
}

/** Steps the state through `seconds` in frames of `frameDt`, with a constant input. */
function run(
  start: BallisticsState,
  input: BallisticsInput,
  seconds: number,
  frameDt = 1 / 60,
): BallisticsState {
  let state = start
  const frames = Math.round(seconds / frameDt)
  for (let i = 0; i < frames; i++) {
    state = stepBallistics(state, input, frameDt)
  }
  return state
}

describe('initialBallistics', () => {
  it('starts every bar at the −120 dB floor with the clip latch open', () => {
    const state = initialBallistics()
    expect(state).toEqual({
      peak: DB_FLOOR,
      rms: DB_FLOOR,
      hold: DB_FLOOR,
      holdAge: 0,
      clipLatched: false,
    })
  })
})

describe('stepBallistics peak', () => {
  it('attacks instantly to a louder input', () => {
    const state = stepBallistics(initialBallistics(), loud(-6), 1 / 60)
    expect(state.peak).toBeCloseTo(-6, 9)
  })

  it('falls at 24 dB per second when the input goes quiet', () => {
    const peaked = stepBallistics(initialBallistics(), loud(0), 0)
    const fallen = stepBallistics(peaked, SILENCE, 0.5)
    expect(fallen.peak).toBeCloseTo(0 - PEAK_FALL_DB_PER_S * 0.5, 9)
  })

  it('is governed by the falling bar while the input is quieter than it', () => {
    const state = stepBallistics({ ...initialBallistics(), peak: -3 }, loud(-40), 0.01)
    expect(state.peak).toBeCloseTo(-3 - PEAK_FALL_DB_PER_S * 0.01, 9)
  })

  it('follows the input once the input is louder than the falling bar', () => {
    const state = stepBallistics({ ...initialBallistics(), peak: -3 }, loud(-2.9), 0.01)
    expect(state.peak).toBeCloseTo(-2.9, 9)
  })

  it('does not fall past the −120 dB floor', () => {
    const state = run({ ...initialBallistics(), peak: -10 }, SILENCE, 10)
    expect(state.peak).toBe(DB_FLOOR)
  })
})

describe('stepBallistics rms', () => {
  it('falls at 12 dB per second, half the peak rate', () => {
    const start: BallisticsState = { ...initialBallistics(), rms: 0 }
    const state = stepBallistics(start, SILENCE, 1)
    expect(state.rms).toBeCloseTo(-RMS_FALL_DB_PER_S, 9)
  })

  it('follows a louder rms input immediately', () => {
    const state = stepBallistics(initialBallistics(), loud(-3, -9), 1 / 60)
    expect(state.rms).toBeCloseTo(-9, 9)
  })
})

describe('stepBallistics peak hold', () => {
  it('raises the hold to a new peak and restarts its age', () => {
    const state = stepBallistics({ ...initialBallistics(), holdAge: 1.2, hold: -20 }, loud(-4), 0.1)
    expect(state.hold).toBeCloseTo(-4, 9)
    expect(state.holdAge).toBe(0)
  })

  it('keeps the hold for 1.5 s after the peak falls away', () => {
    const peaked = stepBallistics(initialBallistics(), loud(-6), 0)
    const afterHold = run(peaked, SILENCE, HOLD_SECONDS - 0.05)
    expect(afterHold.hold).toBeCloseTo(-6, 9)
  })

  it('starts falling the hold after 1.5 s at 24 dB per second', () => {
    const peaked = stepBallistics(initialBallistics(), loud(-6), 0)
    const state = run(peaked, SILENCE, HOLD_SECONDS + 0.5, 0.01)
    expect(state.hold).toBeCloseTo(-6 - PEAK_FALL_DB_PER_S * 0.5, 0)
    expect(state.hold).toBeLessThan(-6)
  })

  it('stops falling at the current input when the input is louder than the hold', () => {
    const start: BallisticsState = { ...initialBallistics(), hold: -30, holdAge: 5 }
    const state = stepBallistics(start, loud(-25), 0.01)
    expect(state.hold).toBeCloseTo(-25, 9)
  })
})

describe('stepBallistics clip latch', () => {
  it('latches when the linear peak reaches 0.9999', () => {
    const state = stepBallistics(initialBallistics(), { peakDb: 0, rmsDb: -3, peakLinear: CLIP_LINEAR }, 0.01)
    expect(state.clipLatched).toBe(true)
  })

  it('does not latch just below 0.9999', () => {
    const state = stepBallistics(initialBallistics(), { peakDb: -0.0009, rmsDb: -3, peakLinear: 0.99989 }, 0.01)
    expect(state.clipLatched).toBe(false)
  })

  it('stays latched after the peak falls away', () => {
    const clipped = stepBallistics(initialBallistics(), { peakDb: 0, rmsDb: 0, peakLinear: 1 }, 0)
    const later = run(clipped, SILENCE, 5)
    expect(later.clipLatched).toBe(true)
    expect(later.peak).toBe(DB_FLOOR)
  })
})

describe('stepBallistics robustness', () => {
  it('treats a negative dt as zero so the meter never runs backwards', () => {
    const start: BallisticsState = { ...initialBallistics(), peak: -5, rms: -5, hold: -5 }
    const state = stepBallistics(start, SILENCE, -10)
    expect(state.peak).toBe(-5)
    expect(state.holdAge).toBe(0)
  })

  it('treats NaN input as silence instead of poisoning the state', () => {
    const state = stepBallistics(initialBallistics(), { peakDb: Number.NaN, rmsDb: Number.NaN, peakLinear: Number.NaN }, 0.01)
    expect(state.peak).toBe(DB_FLOOR)
    expect(state.rms).toBe(DB_FLOOR)
    expect(state.clipLatched).toBe(false)
  })

  it('treats infinite input as silence rather than a stuck peak', () => {
    const state = stepBallistics(initialBallistics(), { peakDb: Number.POSITIVE_INFINITY, rmsDb: 0, peakLinear: 0 }, 0.01)
    expect(state.peak).toBe(DB_FLOOR)
  })

  it('does not mutate the previous state', () => {
    const start = initialBallistics()
    stepBallistics(start, loud(0), 0.1)
    expect(start).toEqual(initialBallistics())
  })
})
