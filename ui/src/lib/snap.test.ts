import { describe, expect, it } from 'vitest'
import { snap, snapCandidates, snapMove } from './snap'
import type { Clip, Project } from './types'

const SR = 48000

function clip(id: number, start: number, length: number): Clip {
  return { id, sourceId: 1, start, offset: 0, length, gainDb: 0, fadeIn: 0, fadeOut: 0 }
}

function projectWith(tracks: Clip[][]): Project {
  return {
    version: 1,
    sampleRate: SR,
    tracks: tracks.map((clips, index) => ({
      id: index + 1,
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

describe('snapCandidates', () => {
  it('includes zero, the playhead and every clip start and end', () => {
    const project = projectWith([[clip(1, 100, 50)], [clip(2, 300, 20)]])

    const candidates = snapCandidates(project, null, 500, null)

    expect(candidates).toEqual([0, 100, 150, 300, 320, 500])
  })

  it('includes loop start and end when a loop region is set', () => {
    const project = projectWith([[]])

    const candidates = snapCandidates(project, null, 0, { start: 40, end: 90 })

    expect(candidates).toEqual([0, 40, 90])
  })

  it('excludes the clip being moved', () => {
    const project = projectWith([[clip(1, 100, 50), clip(2, 300, 20)]])

    const candidates = snapCandidates(project, 1, 0, null)

    expect(candidates).toEqual([0, 300, 320])
  })

  it('returns sorted, deduplicated candidates across tracks', () => {
    const project = projectWith([[clip(1, 200, 100)], [clip(2, 100, 100)]])

    const candidates = snapCandidates(project, null, 200, null)

    expect(candidates).toEqual([0, 100, 200, 300])
  })

  it('returns only zero for an empty project with no playhead movement', () => {
    const candidates = snapCandidates(projectWith([]), null, 0, null)

    expect(candidates).toEqual([0])
  })
})

describe('snap', () => {
  it('snaps to the nearest candidate within the threshold', () => {
    const result = snap([100, 500], 96, 8, true)

    expect(result).toEqual({ frame: 100, target: 100 })
  })

  it('does not snap when every candidate is outside the threshold', () => {
    const result = snap([100, 500], 120, 8, true)

    expect(result).toEqual({ frame: 120, target: null })
  })

  it('picks the closer of two candidates on either side', () => {
    const result = snap([100, 110], 107, 8, true)

    expect(result).toEqual({ frame: 110, target: 110 })
  })

  it('returns the frame unchanged when snapping is disabled', () => {
    const result = snap([100], 101, 8, false)

    expect(result).toEqual({ frame: 101, target: null })
  })

  it('returns the frame unchanged when there are no candidates', () => {
    expect(snap([], 50, 8, true)).toEqual({ frame: 50, target: null })
  })

  it('finds the nearest candidate even when the list is unsorted', () => {
    const result = snap([500, 100, 300], 298, 8, true)

    expect(result.target).toBe(300)
  })

  it('keeps the earlier candidate on an exact tie', () => {
    const result = snap([100, 120], 110, 10, true)

    expect(result.target).toBe(100)
  })
})

describe('snapMove', () => {
  it('snaps the start edge when it is closer than the end edge', () => {
    const result = snapMove([1002], 1000, 500, 8, true)

    expect(result).toEqual({ frame: 1002, target: 1002 })
  })

  it('snaps the end edge and returns the start that puts the end on the candidate', () => {
    const result = snapMove([1500], 1000, 497, 8, true)

    expect(result).toEqual({ frame: 1003, target: 1500 })
  })

  it('prefers the start edge when both edges are equally close', () => {
    const result = snapMove([1000, 1500], 1000, 500, 8, true)

    expect(result).toEqual({ frame: 1000, target: 1000 })
  })

  it('does not snap when no edge is within the threshold', () => {
    const result = snapMove([100], 1000, 500, 8, true)

    expect(result).toEqual({ frame: 1000, target: null })
  })

  it('returns the unchanged start when snapping is disabled', () => {
    const result = snapMove([1002], 1000, 500, 8, false)

    expect(result).toEqual({ frame: 1000, target: null })
  })

  it('never returns a negative start', () => {
    const result = snapMove([0], -3, 500, 8, true)

    expect(result.frame).toBeGreaterThanOrEqual(0)
  })

  it('ignores an end-edge candidate that would push the start below zero', () => {
    const result = snapMove([100], 0, 102, 8, true)

    expect(result).toEqual({ frame: 0, target: null })
  })
})
