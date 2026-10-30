import { describe, expect, it } from 'vitest'
import {
  MIN_CLIP_FRAMES,
  computeFades,
  computeGain,
  computeMove,
  computeSplit,
  computeTrimLeft,
  computeTrimRight,
} from './clipEdit'
import type { Clip } from './types'

/** Clip on the timeline 1000..6000 that starts 200 frames into its source. */
function makeClip(overrides: Partial<Clip> = {}): Clip {
  return {
    id: 1,
    sourceId: 1,
    start: 1000,
    offset: 200,
    length: 5000,
    gainDb: 0,
    fadeIn: 0,
    fadeOut: 0,
    ...overrides,
  }
}

describe('computeMove', () => {
  it('moves the start by the given delta', () => {
    expect(computeMove(makeClip(), 100)).toBe(1100)
  })

  it('moves the start backwards', () => {
    expect(computeMove(makeClip(), -400)).toBe(600)
  })

  it('clamps the new start at zero', () => {
    expect(computeMove(makeClip(), -5000)).toBe(0)
  })

  it('rounds fractional deltas to whole frames', () => {
    expect(computeMove(makeClip(), 10.6)).toBe(1011)
  })

  it('returns null for a no-op move', () => {
    expect(computeMove(makeClip(), 0)).toBeNull()
  })

  it('returns null for a non-finite delta', () => {
    expect(computeMove(makeClip(), Number.NaN)).toBeNull()
  })
})

describe('computeTrimLeft', () => {
  it('moves the start and keeps the clip end fixed', () => {
    const clip = makeClip()

    const geom = computeTrimLeft(clip, 1500)

    expect(geom).toEqual({ start: 1500, offset: 700, length: 4500 })
    expect(geom && geom.start + geom.length).toBe(clip.start + clip.length)
  })

  it('clamps so the offset never goes below zero', () => {
    const geom = computeTrimLeft(makeClip(), 0)

    expect(geom).toEqual({ start: 800, offset: 0, length: 5200 })
  })

  it('clamps so the length never drops below the minimum', () => {
    const clip = makeClip()

    const geom = computeTrimLeft(clip, 99999)

    expect(geom?.length).toBe(MIN_CLIP_FRAMES)
    expect(geom && geom.start + geom.length).toBe(clip.start + clip.length)
  })

  it('returns null when trimming would be a no-op', () => {
    expect(computeTrimLeft(makeClip(), 1000)).toBeNull()
  })

  it('returns null for a non-finite target', () => {
    expect(computeTrimLeft(makeClip(), Number.NaN)).toBeNull()
  })

  it('returns null when the clip is already shorter than the minimum', () => {
    const shortClip = makeClip({ length: 10 })

    expect(computeTrimLeft(shortClip, 1005)).toBeNull()
  })
})

describe('computeTrimRight', () => {
  const sourceFrames = 10000

  it('changes only the length for a right trim', () => {
    const geom = computeTrimRight(makeClip(), 8000, sourceFrames)

    expect(geom).toEqual({ start: 1000, offset: 200, length: 7000 })
  })

  it('clamps so offset plus length never exceeds the source', () => {
    const geom = computeTrimRight(makeClip(), 50000, sourceFrames)

    expect(geom).toEqual({ start: 1000, offset: 200, length: 9800 })
    expect(geom && geom.offset + geom.length).toBe(sourceFrames)
  })

  it('clamps so the length never drops below the minimum', () => {
    const geom = computeTrimRight(makeClip(), 1001, sourceFrames)

    expect(geom?.length).toBe(MIN_CLIP_FRAMES)
  })

  it('returns null when the new end equals the current end', () => {
    expect(computeTrimRight(makeClip(), 6000, sourceFrames)).toBeNull()
  })

  it('returns null when the clip is already shorter than the minimum', () => {
    expect(computeTrimRight(makeClip({ length: 10 }), 1005, sourceFrames)).toBeNull()
  })

  it('returns null when the source is too short to hold the minimum length', () => {
    expect(computeTrimRight(makeClip(), 8000, 100)).toBeNull()
  })

  it('returns null for a non-finite target', () => {
    expect(computeTrimRight(makeClip(), Number.NaN, sourceFrames)).toBeNull()
  })
})

describe('computeFades', () => {
  it('sets the dragged fade-in and keeps fade-out', () => {
    const clip = makeClip({ fadeOut: 100 })

    expect(computeFades(clip, 'in', 300)).toEqual({ fadeIn: 300, fadeOut: 100 })
  })

  it('limits fade-in so the fades together fit in the clip', () => {
    const clip = makeClip({ fadeOut: 4900 })

    expect(computeFades(clip, 'in', 300)).toEqual({ fadeIn: 100, fadeOut: 4900 })
  })

  it('limits fade-out so the fades together fit in the clip', () => {
    const clip = makeClip({ fadeIn: 1000 })

    expect(computeFades(clip, 'out', 99999)).toEqual({ fadeIn: 1000, fadeOut: 4000 })
  })

  it('clamps a negative request to zero', () => {
    const clip = makeClip({ fadeIn: 200 })

    expect(computeFades(clip, 'in', -50)).toEqual({ fadeIn: 0, fadeOut: 0 })
  })

  it('returns null when the fade does not change', () => {
    const clip = makeClip({ fadeIn: 300 })

    expect(computeFades(clip, 'in', 300)).toBeNull()
  })

  it('returns null for a non-finite request', () => {
    expect(computeFades(makeClip(), 'out', Number.NaN)).toBeNull()
  })
})

describe('computeGain', () => {
  it('lowers gain by 0.1 dB per pixel dragged down', () => {
    expect(computeGain(0, 100, false)).toBe(-10)
  })

  it('raises gain when dragged up', () => {
    expect(computeGain(0, -30, false)).toBe(3)
  })

  it('uses one hundredth of a dB per pixel when fine', () => {
    expect(computeGain(0, 100, true)).toBe(-1)
  })

  it('clamps at the maximum clip gain', () => {
    expect(computeGain(0, -10000, false)).toBe(24)
  })

  it('clamps at the minimum gain', () => {
    expect(computeGain(0, 10000, false)).toBe(-96)
  })

  it('rounds to a tenth of a dB', () => {
    expect(computeGain(0, 3.3, false)).toBe(-0.3)
    expect(computeGain(0, 0.4, false)).toBe(0)
  })

  it('normalises negative zero to zero', () => {
    expect(Object.is(computeGain(0, 0, false), 0)).toBe(true)
    expect(Object.is(computeGain(-0.04, 0, false), 0)).toBe(true)
  })

  it('treats a non-finite drag distance as no movement', () => {
    expect(computeGain(-6, Number.NaN, false)).toBe(-6)
  })
})

describe('computeSplit', () => {
  it('returns the split frame strictly inside the clip', () => {
    expect(computeSplit(makeClip(), 2000)).toBe(2000)
  })

  it('accepts a split one frame after the start and one frame before the end', () => {
    expect(computeSplit(makeClip(), 1001)).toBe(1001)
    expect(computeSplit(makeClip(), 5999)).toBe(5999)
  })

  it('returns null at the start edge', () => {
    expect(computeSplit(makeClip(), 1000)).toBeNull()
  })

  it('returns null at the end edge', () => {
    expect(computeSplit(makeClip(), 6000)).toBeNull()
  })

  it('returns null when rounding lands the split on an edge', () => {
    expect(computeSplit(makeClip(), 5999.6)).toBeNull()
  })

  it('returns null for a non-finite split point', () => {
    expect(computeSplit(makeClip(), Number.NaN)).toBeNull()
  })

  it('returns null for a zero-length clip', () => {
    expect(computeSplit(makeClip({ length: 0 }), 1000)).toBeNull()
  })
})
