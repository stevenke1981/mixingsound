// Meter ballistics (ui-spec §4.3). One channel per state; the caller steps it once per animation frame.
// Input arrives at ~30 Hz, so `dt` is the time since the previous frame, not since the previous input.

import { DB_FLOOR } from './dbScale'

export const PEAK_FALL_DB_PER_S = 24
export const RMS_FALL_DB_PER_S = 12
export const HOLD_SECONDS = 1.5
/** Linear peak at or above this latches the clip LED (≈ 0 dBFS). */
export const CLIP_LINEAR = 0.9999

export interface BallisticsState {
  /** Peak bar, dBFS. Instant attack, falls at PEAK_FALL_DB_PER_S. */
  peak: number
  /** RMS bar, dBFS. Falls at RMS_FALL_DB_PER_S. */
  rms: number
  /** Peak-hold tick, dBFS. Holds for HOLD_SECONDS, then falls at PEAK_FALL_DB_PER_S. */
  hold: number
  /** Seconds since the hold was last raised. */
  holdAge: number
  /** Latched once the peak reached CLIP_LINEAR. Cleared by the store (user click / new project). */
  clipLatched: boolean
}

export interface BallisticsInput {
  peakDb: number
  rmsDb: number
  peakLinear: number
}

export function initialBallistics(): BallisticsState {
  return { peak: DB_FLOOR, rms: DB_FLOOR, hold: DB_FLOOR, holdAge: 0, clipLatched: false }
}

/** Keeps dB inputs finite and at or above the floor; NaN and ±∞ read as silence. */
function sanitizeDb(db: number): number {
  return Number.isFinite(db) ? Math.max(DB_FLOOR, db) : DB_FLOOR
}

/**
 * Advances one channel by `dtSeconds`. Pure: returns a new state.
 * Negative `dtSeconds` is treated as 0 so a clock glitch cannot run the meter backwards.
 */
export function stepBallistics(
  state: BallisticsState,
  input: BallisticsInput,
  dtSeconds: number,
): BallisticsState {
  const dt = Math.max(0, dtSeconds)
  const peakIn = sanitizeDb(input.peakDb)
  const rmsIn = sanitizeDb(input.rmsDb)

  const peak = Math.max(peakIn, state.peak - PEAK_FALL_DB_PER_S * dt)
  const rms = Math.max(rmsIn, state.rms - RMS_FALL_DB_PER_S * dt)

  let hold = state.hold
  let holdAge = state.holdAge
  if (peakIn >= state.hold) {
    hold = peakIn
    holdAge = 0
  } else {
    holdAge += dt
    if (holdAge > HOLD_SECONDS) {
      hold = Math.max(peakIn, state.hold - PEAK_FALL_DB_PER_S * dt)
    }
  }

  const clipLatched = state.clipLatched || input.peakLinear >= CLIP_LINEAR
  return { peak, rms, hold, holdAge, clipLatched }
}
