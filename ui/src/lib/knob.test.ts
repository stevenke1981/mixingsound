import { describe, expect, it } from 'vitest'
import {
  DRAG_PX_FULL_RANGE,
  KNOB_MAX_DEG,
  KNOB_MIN_DEG,
  angleToPan,
  clampPan,
  dragStep,
  keyStep,
  panAriaValue,
  panShortLabel,
  panSpokenLabel,
  panToAngle,
  wheelStep,
} from './knob'

describe('clampPan', () => {
  it('keeps values inside −1…+1 unchanged', () => {
    expect(clampPan(0.25)).toBe(0.25)
  })

  it('clamps beyond the range to the hard ends', () => {
    expect(clampPan(-3)).toBe(-1)
    expect(clampPan(3)).toBe(1)
  })

  it('treats NaN as centre', () => {
    expect(clampPan(Number.NaN)).toBe(0)
  })
})

describe('panToAngle', () => {
  it('maps hard left to −135°', () => {
    expect(panToAngle(-1)).toBe(KNOB_MIN_DEG)
  })

  it('maps hard right to +135°', () => {
    expect(panToAngle(1)).toBe(KNOB_MAX_DEG)
  })

  it('maps centre to 0°, straight up', () => {
    expect(panToAngle(0)).toBeCloseTo(0, 9)
  })

  it('sweeps 270° in total, linearly', () => {
    expect(panToAngle(0.5) - panToAngle(0)).toBeCloseTo(67.5, 9)
  })

  it('clamps out-of-range pan to the sweep ends', () => {
    expect(panToAngle(-9)).toBe(KNOB_MIN_DEG)
    expect(panToAngle(9)).toBe(KNOB_MAX_DEG)
  })
})

describe('angleToPan', () => {
  it.each([-135, -67.5, 0, 45, 135])('inverts panToAngle at %s°', (deg) => {
    expect(panToAngle(angleToPan(deg))).toBeCloseTo(deg, 9)
  })

  it('clamps angles beyond the sweep', () => {
    expect(angleToPan(-200)).toBe(-1)
    expect(angleToPan(200)).toBe(1)
  })

  it('treats NaN as centre', () => {
    expect(angleToPan(Number.NaN)).toBe(0)
  })
})

describe('dragStep', () => {
  it('moves 150 px for the full −1…+1 range', () => {
    expect(dragStep(-1, -DRAG_PX_FULL_RANGE, false)).toBeCloseTo(1, 9)
    expect(dragStep(1, DRAG_PX_FULL_RANGE, false)).toBeCloseTo(-1, 9)
  })

  it('raises the value when the pointer moves up (negative dy)', () => {
    expect(dragStep(0, -15, false)).toBeCloseTo(0.2, 9)
  })

  it('lowers the value when the pointer moves down (positive dy)', () => {
    expect(dragStep(0, 15, false)).toBeCloseTo(-0.2, 9)
  })

  it('scales the movement by 0.1 with Shift (fine)', () => {
    expect(dragStep(0, -150, true)).toBeCloseTo(0.2, 9)
    expect(dragStep(0, -15, true)).toBeCloseTo(0.02, 9)
  })

  it('clamps at the hard ends', () => {
    expect(dragStep(0.9, -1000, false)).toBe(1)
    expect(dragStep(-0.9, 1000, false)).toBe(-1)
  })

  it('returns the clamped current value for a non-finite movement', () => {
    expect(dragStep(0.4, Number.NaN, false)).toBe(0.4)
  })

  it('is incremental: two half drags equal one full drag', () => {
    const halfway = dragStep(0, -30, false)
    expect(dragStep(halfway, -30, false)).toBeCloseTo(dragStep(0, -60, false), 9)
    expect(dragStep(0, -60, false)).toBeCloseTo(0.8, 9)
  })
})

describe('wheelStep', () => {
  it('raises by 0.02 for one notch up (negative deltaY)', () => {
    expect(wheelStep(0, -100, false)).toBeCloseTo(0.02, 9)
  })

  it('lowers by 0.02 for one notch down (positive deltaY)', () => {
    expect(wheelStep(0, 100, false)).toBeCloseTo(-0.02, 9)
  })

  it('uses 0.005 per notch with Shift', () => {
    expect(wheelStep(0, -100, true)).toBeCloseTo(0.005, 9)
    expect(wheelStep(0, 100, true)).toBeCloseTo(-0.005, 9)
  })

  it('ignores a zero or non-finite deltaY', () => {
    expect(wheelStep(0.3, 0, false)).toBe(0.3)
    expect(wheelStep(0.3, Number.NaN, false)).toBe(0.3)
  })

  it('clamps at the ends', () => {
    expect(wheelStep(0.99, -100, false)).toBe(1)
    expect(wheelStep(-0.99, 100, false)).toBe(-1)
  })
})

describe('keyStep', () => {
  it('moves 0.02 on ArrowRight and ArrowUp', () => {
    expect(keyStep(0, 'ArrowRight', false)).toBeCloseTo(0.02, 9)
    expect(keyStep(0, 'ArrowUp', false)).toBeCloseTo(0.02, 9)
  })

  it('moves −0.02 on ArrowLeft and ArrowDown', () => {
    expect(keyStep(0, 'ArrowLeft', false)).toBeCloseTo(-0.02, 9)
    expect(keyStep(0, 'ArrowDown', false)).toBeCloseTo(-0.02, 9)
  })

  it('moves 0.005 with Shift', () => {
    expect(keyStep(0, 'ArrowRight', true)).toBeCloseTo(0.005, 9)
    expect(keyStep(0, 'ArrowLeft', true)).toBeCloseTo(-0.005, 9)
  })

  it('moves ±0.1 on PgUp and PgDn', () => {
    expect(keyStep(0, 'PageUp', false)).toBeCloseTo(0.1, 9)
    expect(keyStep(0, 'PageDown', false)).toBeCloseTo(-0.1, 9)
  })

  it('jumps to hard left on Home and hard right on End', () => {
    expect(keyStep(0.3, 'Home', false)).toBe(-1)
    expect(keyStep(-0.3, 'End', false)).toBe(1)
  })

  it('returns to centre on the 0 key', () => {
    expect(keyStep(0.7, '0', false)).toBe(0)
  })

  it('clamps steps at the hard ends', () => {
    expect(keyStep(1, 'ArrowRight', false)).toBe(1)
    expect(keyStep(-1, 'PageDown', false)).toBe(-1)
  })

  it('returns null for keys the knob does not handle', () => {
    expect(keyStep(0, 'Enter', false)).toBeNull()
    expect(keyStep(0, 'a', false)).toBeNull()
  })
})

describe('pan readouts', () => {
  it.each([
    [-0.32, -32, 'L 32', 'Left 32'],
    [0, 0, 'C', 'Center'],
    [0.18, 18, 'R 18', 'Right 18'],
    [0.004, 0, 'C', 'Center'],
    [-1, -100, 'L 100', 'Left 100'],
  ])('pan %s → aria %s, short %j, spoken %j', (pan, aria, short, spoken) => {
    expect(panAriaValue(pan)).toBe(aria)
    expect(panShortLabel(pan)).toBe(short)
    expect(panSpokenLabel(pan)).toBe(spoken)
  })

  it('never reports −0 as the ARIA value', () => {
    expect(Object.is(panAriaValue(-0.001), -0)).toBe(false)
    expect(panAriaValue(-0.001)).toBe(0)
  })
})
