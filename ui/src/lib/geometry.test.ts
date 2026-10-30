import { describe, expect, it } from 'vitest'
import {
  clampFpp,
  defaultFpp,
  fitFpp,
  fppToSlider,
  frameToX,
  scrollExtent,
  sliderToFpp,
  visibleRange,
  xToFrame,
  zoomAt,
  ZOOM_STEP,
} from './geometry'

const SR = 48000

describe('clampFpp', () => {
  it('clamps values below one frame per pixel up to 1', () => {
    expect(clampFpp(0.25, SR)).toBe(1)
  })

  it('clamps values above sampleRate times four down to that maximum', () => {
    expect(clampFpp(1e9, SR)).toBe(SR * 4)
  })

  it('leaves in-range values unchanged', () => {
    expect(clampFpp(480, SR)).toBe(480)
  })

  it('falls back to the default zoom when the input is not finite', () => {
    expect(clampFpp(Number.NaN, SR)).toBe(defaultFpp(SR))
    expect(clampFpp(Number.POSITIVE_INFINITY, SR)).toBe(defaultFpp(SR))
  })
})

describe('frame and pixel conversion', () => {
  it('maps frame to x and back through xToFrame', () => {
    const x = frameToX(12000, 3000, 100)
    expect(x).toBe(90)
    expect(xToFrame(x, 3000, 100)).toBe(12000)
  })
})

describe('zoomAt', () => {
  it('keeps the frame under the anchor fixed when zooming out', () => {
    const before = { fpp: 100, scroll: 50000 }
    const anchorX = 300
    const anchorFrame = xToFrame(anchorX, before.scroll, before.fpp)

    const after = zoomAt(before.fpp, before.scroll, ZOOM_STEP, anchorX, SR)

    expect(after.fpp).toBeCloseTo(125)
    expect(xToFrame(anchorX, after.scrollFrames, after.fpp)).toBeCloseTo(anchorFrame)
  })

  it('keeps the frame under the anchor fixed when zooming in', () => {
    const anchorX = 150
    const anchorFrame = xToFrame(anchorX, 40000, 200)

    const after = zoomAt(200, 40000, 1 / ZOOM_STEP, anchorX, SR)

    expect(xToFrame(anchorX, after.scrollFrames, after.fpp)).toBeCloseTo(anchorFrame)
  })

  it('clamps scroll at zero when keeping the anchor would need negative scroll', () => {
    const after = zoomAt(100, 0, ZOOM_STEP, 100, SR)

    expect(after.scrollFrames).toBe(0)
    expect(after.fpp).toBeCloseTo(125)
  })

  it('clamps the resulting fpp to the allowed range', () => {
    const tooFine = zoomAt(1.1, 0, 0.1, 0, SR)
    const tooCoarse = zoomAt(SR * 4, 0, 10, 0, SR)

    expect(tooFine.fpp).toBe(1)
    expect(tooCoarse.fpp).toBe(SR * 4)
  })
})

describe('fitFpp', () => {
  it('fits project end with 5 percent headroom into the viewport', () => {
    const projectEnd = 10 * SR
    const viewportW = 1000

    const fpp = fitFpp(projectEnd, viewportW, SR)

    expect(fpp).toBeCloseTo((projectEnd * 1.05) / viewportW)
  })

  it('applies a one second minimum span for an empty project', () => {
    const fpp = fitFpp(0, 480, SR)

    expect(fpp).toBeCloseTo(SR / 480)
  })

  it('returns the default zoom when the viewport has no width', () => {
    expect(fitFpp(10 * SR, 0, SR)).toBe(defaultFpp(SR))
    expect(fitFpp(10 * SR, -50, SR)).toBe(defaultFpp(SR))
  })
})

describe('zoom slider mapping', () => {
  it('maps t = 1 to the most zoomed-in position (one frame per pixel)', () => {
    expect(sliderToFpp(1, SR)).toBe(1)
  })

  it('maps t = 0 to the most zoomed-out position (sampleRate times four)', () => {
    expect(sliderToFpp(0, SR)).toBeCloseTo(SR * 4)
  })

  it('places t = 0.5 at the geometric mean of the two bounds', () => {
    expect(sliderToFpp(0.5, SR)).toBeCloseTo(Math.sqrt(SR * 4))
  })

  it('clamps slider positions outside [0, 1]', () => {
    expect(sliderToFpp(2, SR)).toBe(1)
    expect(sliderToFpp(-1, SR)).toBeCloseTo(SR * 4)
  })

  it('round-trips slider position through fpp', () => {
    for (const t of [0, 0.1, 0.37, 0.5, 0.9, 1]) {
      expect(fppToSlider(sliderToFpp(t, SR), SR)).toBeCloseTo(t, 10)
    }
  })
})

describe('visibleRange and scrollExtent', () => {
  it('returns the frame span covered by the viewport', () => {
    expect(visibleRange(1000, 10, 200)).toEqual([1000, 3000])
  })

  it('reserves thirty seconds past the project end', () => {
    const extent = scrollExtent(5 * SR, SR, 0, 100, 800)

    expect(extent).toBe(35 * SR)
  })

  it('extends beyond the runway when the view end is further right', () => {
    const scroll = 2_000_000
    const viewEnd = scroll + 800 * 100
    const extent = scrollExtent(0, SR, scroll, 100, 800)

    expect(viewEnd).toBeGreaterThan(30 * SR)
    expect(extent).toBe(viewEnd)
  })
})
