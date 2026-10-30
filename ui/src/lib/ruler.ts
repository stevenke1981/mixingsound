// Ruler ticks (ui-spec §2.4). Pure. STUB — WP-B1 implements.

export interface Tick {
  frame: number
  major: boolean
  /** Label for major ticks only. */
  label: string | null
}
/** Step from [1,2,5]×10^n seconds with labels ≥ 70 px apart; minor = step/5 (step/2 when step is 5×). */
export function tickStep(fpp: number, sampleRate: number): { majorSec: number; minorSec: number } {
  return { majorSec: 1, minorSec: 0.2 }
}
export function ticks(scrollFrames: number, fpp: number, viewportW: number, sampleRate: number): Tick[] {
  return []
}
