import { describe, expect, it } from 'vitest'
import { hitTest, laneAt } from './hitTest'
import type { Clip, Project } from './types'

const SR = 48000
/** Scroll 0 and one frame per pixel keep the arithmetic readable: x equals the frame number. */
const FPP = 1
const SCROLL = 0
/** Lane 0 spans y 0..72; the clip rect spans y 4..68; the header strip is y 4..20. */
const CLIP_Y = 30

function clip(id: number, start: number, length: number, extra: Partial<Clip> = {}): Clip {
  return { id, sourceId: 1, start, offset: 0, length, gainDb: 0, fadeIn: 0, fadeOut: 0, ...extra }
}

function projectWith(tracks: Clip[][]): Project {
  return {
    version: 1,
    sampleRate: SR,
    tracks: tracks.map((clips, index) => ({
      id: 10 + index,
      name: `T${index + 1}`,
      gainDb: 0,
      pan: 0,
      mute: false,
      solo: false,
      color: '#000000',
      clips,
    })),
    master: { gainDb: 0 },
    sources: [],
    nextId: 100,
  }
}

function hit(project: Project, x: number, y: number, selected: number | null = null) {
  return hitTest(project, x, y, SCROLL, FPP, selected)
}

describe('hitTest body and empty areas', () => {
  it('returns the body of a clip under the pointer', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 200, CLIP_Y)).toEqual({ kind: 'body', clipId: 1, trackId: 10 })
  })

  it('returns null in the empty part of a lane', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 50, CLIP_Y)).toBeNull()
  })

  it('returns null in the vertical inset above and below the clip rect', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 200, 2)).toBeNull()
    expect(hit(project, 200, 70)).toBeNull()
  })

  it('returns null for a lane index past the last track', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 200, 80)).toBeNull()
  })

  it('returns null when the project has no tracks', () => {
    expect(hit(projectWith([]), 200, CLIP_Y)).toBeNull()
  })

  it('returns null for a non-positive zoom', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hitTest(project, 200, CLIP_Y, SCROLL, 0, null)).toBeNull()
  })

  it('resolves the second lane using its own track id', () => {
    const project = projectWith([[], [clip(7, 100, 200)]])

    expect(hit(project, 200, 72 + CLIP_Y)).toEqual({ kind: 'body', clipId: 7, trackId: 11 })
  })

  it('applies the scroll offset when mapping x to frames', () => {
    const project = projectWith([[clip(1, 1000, 200)]])

    const result = hitTest(project, 150, CLIP_Y, 900, FPP, null)

    expect(result).toEqual({ kind: 'body', clipId: 1, trackId: 10 })
  })
})

describe('hitTest edges', () => {
  it('reports the left edge within 6 px of the start', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 100, CLIP_Y)?.kind).toBe('edgeL')
    expect(hit(project, 105, CLIP_Y)?.kind).toBe('edgeL')
  })

  it('reports the right edge within 6 px of the end', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 294, CLIP_Y)?.kind).toBe('edgeR')
    expect(hit(project, 299, CLIP_Y)?.kind).toBe('edgeR')
  })

  it('treats 6 px from the left as body, not edge', () => {
    const project = projectWith([[clip(1, 100, 200)]])

    expect(hit(project, 106, CLIP_Y)?.kind).toBe('body')
  })

  it('uses a quarter of the width as the edge zone for clips narrower than 24 px', () => {
    const project = projectWith([[clip(2, 500, 20)]])

    expect(hit(project, 504, CLIP_Y)?.kind).toBe('edgeL')
    expect(hit(project, 510, CLIP_Y)?.kind).toBe('body')
    expect(hit(project, 515, CLIP_Y)?.kind).toBe('edgeR')
  })

  it('reports an edge of one clip over the body of an overlapping later clip', () => {
    const project = projectWith([[clip(1, 100, 200), clip(2, 250, 200)]])

    expect(hit(project, 294, CLIP_Y)).toEqual({ kind: 'edgeR', clipId: 1, trackId: 10 })
  })

  it('does not report an edge for the left side of an early clip inside a later clip body', () => {
    const project = projectWith([[clip(1, 100, 200), clip(2, 250, 200)]])

    expect(hit(project, 280, CLIP_Y)).toEqual({ kind: 'body', clipId: 2, trackId: 10 })
  })
})

describe('hitTest fade handles', () => {
  it('reports the fade-in handle of the selected clip at its fade end', () => {
    const project = projectWith([[clip(1, 100, 200, { fadeIn: 50 })]])
    const handleY = 4 + 16 + 2

    expect(hit(project, 150, handleY, 1)).toEqual({ kind: 'fadeIn', clipId: 1, trackId: 10 })
  })

  it('reports the fade-out handle of the selected clip at its fade start', () => {
    const project = projectWith([[clip(1, 100, 200, { fadeOut: 40 })]])
    const handleY = 4 + 16 + 2

    expect(hit(project, 260, handleY, 1)).toEqual({ kind: 'fadeOut', clipId: 1, trackId: 10 })
  })

  it('ignores fade handles of a clip that is neither selected nor hovered', () => {
    const project = projectWith([[clip(1, 100, 200, { fadeIn: 50 })]])
    const handleY = 4 + 16 + 2

    expect(hit(project, 150, handleY, null)).toEqual({ kind: 'body', clipId: 1, trackId: 10 })
  })

  it('does not report a fade handle inside the header strip', () => {
    const project = projectWith([[clip(1, 100, 200, { fadeIn: 50 })]])

    expect(hit(project, 150, 10, 1)?.kind).toBe('body')
  })

  it('prefers a fade handle over an edge of another clip', () => {
    const project = projectWith([[clip(1, 100, 200, { fadeOut: 40 }), clip(2, 258, 200)]])
    const handleY = 4 + 16 + 2

    expect(hit(project, 260, handleY, 1)).toEqual({ kind: 'fadeOut', clipId: 1, trackId: 10 })
  })

  it('prefers an edge over the body of a later clip', () => {
    const project = projectWith([[clip(1, 100, 200), clip(2, 90, 300)]])

    expect(hit(project, 101, CLIP_Y)).toEqual({ kind: 'edgeL', clipId: 1, trackId: 10 })
  })
})

describe('laneAt', () => {
  it('maps content y to the lane index', () => {
    expect(laneAt(0, 3)).toBe(0)
    expect(laneAt(71.9, 3)).toBe(0)
    expect(laneAt(72, 3)).toBe(1)
    expect(laneAt(150, 3)).toBe(2)
  })

  it('returns null for an index past the last track', () => {
    expect(laneAt(72, 1)).toBeNull()
  })

  it('returns null for negative or non-finite y', () => {
    expect(laneAt(-1, 3)).toBeNull()
    expect(laneAt(Number.NaN, 3)).toBeNull()
  })
})
