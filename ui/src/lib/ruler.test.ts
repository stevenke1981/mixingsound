import { describe, expect, it } from 'vitest'
import { ticks, tickStep } from './ruler'

const SR = 48000
/** Pixels per second for a given fpp: sampleRate / fpp. */
function fppForPxPerSecond(pxPerSecond: number): number {
  return SR / pxPerSecond
}

describe('tickStep', () => {
  it('picks one second as the major step when 100 px per second leaves room for labels', () => {
    const step = tickStep(SR / 100, SR)

    expect(step).toEqual({ majorSec: 1, minorSec: 0.2 })
  })

  it('skips steps whose labels would sit closer than 70 px', () => {
    const step = tickStep(fppForPxPerSecond(50), SR)

    expect(step.majorSec).toBe(2)
    expect(step.minorSec).toBeCloseTo(0.4)
  })

  it('uses half the step as the minor interval for 5× steps', () => {
    const step = tickStep(fppForPxPerSecond(20), SR)

    expect(step.majorSec).toBe(5)
    expect(step.minorSec).toBeCloseTo(2.5)
  })

  it('chooses sub-second steps when zoomed in', () => {
    const step = tickStep(fppForPxPerSecond(1000), SR)

    expect(step.majorSec).toBeCloseTo(0.1)
    expect(step.minorSec).toBeCloseTo(0.02)
  })

  it('keeps adjacent major labels at least 70 px apart at every zoom', () => {
    for (const pxPerSecond of [0.5, 3, 17, 90, 400, 2500, 40000]) {
      const { majorSec } = tickStep(fppForPxPerSecond(pxPerSecond), SR)

      expect(majorSec * pxPerSecond).toBeGreaterThanOrEqual(70)
    }
  })
})

describe('ticks', () => {
  it('labels whole-second majors as m:ss and marks minors without labels', () => {
    const result = ticks(0, SR / 100, 300, SR)

    const majors = result.filter((tick) => tick.major)
    expect(majors.map((tick) => tick.label)).toEqual(['0:00', '0:01', '0:02', '0:03'])
    expect(result.filter((tick) => !tick.major).every((tick) => tick.label === null)).toBe(true)
  })

  it('places five minor ticks between majors for a 1 s step', () => {
    const result = ticks(0, SR / 100, 300, SR)

    const firstMajorIndex = result.findIndex((tick) => tick.major && tick.frame > 0)
    expect(result.slice(0, firstMajorIndex)).toHaveLength(5)
  })

  it('labels sub-second steps as m:ss.mmm', () => {
    const result = ticks(0, SR / 1000, 400, SR)

    const labels = result.filter((tick) => tick.major).map((tick) => tick.label)
    expect(labels).toContain('0:00.000')
    expect(labels).toContain('0:00.100')
    expect(labels).toContain('0:00.200')
  })

  it('formats minutes without padding and seconds with two digits', () => {
    const scroll = 61 * SR

    const labels = ticks(scroll, SR / 100, 100, SR)
      .filter((tick) => tick.major)
      .map((tick) => tick.label)

    expect(labels).toEqual(['1:01', '1:02'])
  })

  it('only emits ticks inside the visible frame range', () => {
    const scroll = 2.5 * SR
    const fpp = SR / 100
    const viewportW = 250
    const end = scroll + viewportW * fpp

    const result = ticks(scroll, fpp, viewportW, SR)

    expect(result.length).toBeGreaterThan(0)
    for (const tick of result) {
      expect(tick.frame).toBeGreaterThanOrEqual(scroll)
      expect(tick.frame).toBeLessThanOrEqual(end)
    }
  })

  it('returns no ticks for an empty viewport', () => {
    expect(ticks(0, SR / 100, 0, SR)).toEqual([])
  })

  it('returns no ticks for a non-positive zoom', () => {
    expect(ticks(0, 0, 300, SR)).toEqual([])
  })

  it('returns no ticks for a non-finite scroll', () => {
    expect(ticks(Number.NaN, SR / 100, 300, SR)).toEqual([])
  })

  it('caps the tick count at extreme zoom-in so the call always terminates', () => {
    const result = ticks(0, 1, 100000, SR)

    expect(result.length).toBeGreaterThan(0)
    expect(result.length).toBeLessThanOrEqual(2000)
  })

  it('caps the tick count at extreme zoom-out', () => {
    const result = ticks(0, SR * 4, 100000, SR)

    expect(result.length).toBeLessThanOrEqual(2000)
  })
})
