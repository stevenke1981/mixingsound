import { describe, expect, it } from 'vitest'
import { LRU_CAP, MAX_IN_FLIGHT, TILE_COLS, WaveformCache, quantizeFpp, type WaveFetcher } from './waveformCache'
import type { WaveformColumns } from './types'

interface FakeCall {
  sourceId: number
  start: number
  end: number
  columns: number
  settled: boolean
  resolve: (columns: WaveformColumns) => void
  reject: (error: unknown) => void
}

/** Every fetch stays pending until the test settles it, so in-flight state is observable. */
function createFakeFetcher() {
  const calls: FakeCall[] = []
  const fetcher: WaveFetcher = (sourceId, start, end, columns) =>
    new Promise<WaveformColumns>((resolve, reject) => {
      const call: FakeCall = {
        sourceId,
        start,
        end,
        columns,
        settled: false,
        resolve: (value) => {
          call.settled = true
          resolve(value)
        },
        reject: (error) => {
          call.settled = true
          reject(error)
        },
      }
      calls.push(call)
    })
  return { calls, fetcher }
}

/**
 * Lets promise callbacks run, including chained pump work. Microtasks only: the cache has no timers,
 * and macrotask flushes are too slow for the 401-tile LRU loops on Windows timers.
 */
async function flush(): Promise<void> {
  for (let tick = 0; tick < 10; tick++) await Promise.resolve()
}

/** Resolves every open call (and any calls they start) until none remain open. */
async function settleAll(calls: FakeCall[], columns: WaveformColumns): Promise<void> {
  for (let round = 0; round < 1000; round++) {
    const open = calls.filter((call) => !call.settled)
    if (open.length === 0) return
    for (const call of open) call.resolve(columns)
    await flush()
  }
}

/** A full tile of identical peaks `[-value, value]`. */
function uniformTile(value: number): WaveformColumns {
  return Array.from({ length: TILE_COLS }, () => [-value, value] as [number, number])
}

describe('quantizeFpp', () => {
  it('keeps exact powers of two unchanged', () => {
    expect(quantizeFpp(1)).toBe(1)
    expect(quantizeFpp(2)).toBe(2)
    expect(quantizeFpp(4)).toBe(4)
  })

  it('snaps to the nearest quarter-octave step', () => {
    expect(quantizeFpp(100)).toBe(108)
    expect(quantizeFpp(3)).toBe(3)
  })

  it('never returns less than one frame per column', () => {
    expect(quantizeFpp(0.5)).toBe(1)
  })

  it('falls back to one frame for non-finite or non-positive zoom', () => {
    expect(quantizeFpp(Number.NaN)).toBe(1)
    expect(quantizeFpp(0)).toBe(1)
    expect(quantizeFpp(Number.POSITIVE_INFINITY)).toBe(1)
  })
})

describe('WaveformCache requests', () => {
  it('returns null and requests the covering tile when nothing is loaded', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    const result = cache.range(7, 0, 100, 1, 50)

    expect(result).toBeNull()
    expect(calls).toHaveLength(1)
    expect(calls[0]).toMatchObject({ sourceId: 7, start: 0, end: TILE_COLS, columns: TILE_COLS })
  })

  it('sizes tiles from the quantised zoom', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    cache.range(1, 1500, 1600, 2, 0)

    expect(calls[0]).toMatchObject({ start: 1024, end: 2048 })
  })

  it('requests nothing for an empty or inverted range', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    expect(cache.range(1, 100, 100, 1, 0)).toBeNull()
    expect(cache.range(1, 200, 100, 1, 0)).toBeNull()
    expect(calls).toHaveLength(0)
  })

  it('never has more than MAX_IN_FLIGHT fetches outstanding', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    cache.range(1, 0, 10 * TILE_COLS, 1, 0)

    expect(MAX_IN_FLIGHT).toBe(4)
    expect(calls).toHaveLength(MAX_IN_FLIGHT)
    expect(calls.filter((call) => !call.settled)).toHaveLength(MAX_IN_FLIGHT)
  })

  it('starts the next queued tile when one in flight settles', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, 10 * TILE_COLS, 1, 0)

    calls[0].resolve(uniformTile(0.1))
    await flush()

    expect(calls).toHaveLength(MAX_IN_FLIGHT + 1)
  })

  it('does not request a tile again while it is in flight', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    cache.range(1, 0, 100, 1, 0)
    cache.range(1, 0, 100, 1, 0)
    cache.range(1, 10, 20, 1, 0)

    expect(calls).toHaveLength(1)
  })

  it('requests the nearest-to-centre tile first', () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const centre = 9 * TILE_COLS + TILE_COLS / 2

    cache.range(1, 0, 10 * TILE_COLS, 1, centre)

    expect(calls.map((call) => call.start)).toEqual([9, 8, 7, 6].map((index) => index * TILE_COLS))
  })
})

describe('WaveformCache results', () => {
  it('fires onTile on arrival and then answers from the loaded tile', async () => {
    const { calls, fetcher } = createFakeFetcher()
    let arrivals = 0
    const cache = new WaveformCache(fetcher, () => {
      arrivals += 1
    })
    cache.range(1, 0, 100, 1, 0)

    calls[0].resolve(uniformTile(0.5))
    await flush()

    expect(arrivals).toBe(1)
    expect(cache.range(1, 0, 100, 1, 0)).toEqual([-0.5, 0.5])
  })

  it('takes min and max across the tile columns that cover the range', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const first = uniformTile(0.2)
    first[511] = [-0.9, 0.1]
    cache.range(1, 500, 520, 1, 510)

    calls[0].resolve(first)
    calls[1].resolve(uniformTile(0.7))
    await flush()

    expect(cache.range(1, 500, 520, 1, 510)).toEqual([-0.9, 0.7])
  })

  it('aggregates at the quantised zoom, one column per fppQ frames', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const tile = uniformTile(0.1)
    tile[1] = [-0.5, 0.9]
    cache.range(1, 0, 4, 2, 0)

    calls[0].resolve(tile)
    await flush()

    expect(cache.range(1, 0, 4, 2, 0)).toEqual([-0.5, 0.9])
  })

  it('returns a partial result from loaded columns while other tiles load', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const first = uniformTile(0.2)
    first[511] = [-0.9, 0.1]
    cache.range(1, 0, TILE_COLS, 1, 0)
    calls[0].resolve(first)
    await flush()

    const result = cache.range(1, 500, 1024, 1, 0)

    expect(result).toEqual([-0.9, 0.2])
    expect(calls).toHaveLength(2)
  })

  it('returns null when covering tiles are all still loading', () => {
    const { fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})

    expect(cache.range(1, 0, 100, 1, 0)).toBeNull()
  })

  it('keeps tiles of different sources apart', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, 100, 1, 0)
    calls[0].resolve(uniformTile(0.3))
    await flush()

    expect(cache.range(2, 0, 100, 1, 0)).toBeNull()
    expect(calls.at(-1)).toMatchObject({ sourceId: 2 })
  })
})

describe('WaveformCache failures', () => {
  it('drops a rejected tile from in-flight, marks it failed and does not retry it', async () => {
    const { calls, fetcher } = createFakeFetcher()
    let arrivals = 0
    const cache = new WaveformCache(fetcher, () => {
      arrivals += 1
    })
    cache.range(1, 0, 100, 1, 0)

    calls[0].reject(new Error('decode failed'))
    await flush()
    const retry = cache.range(1, 0, 100, 1, 0)

    expect(cache.failedTileCount).toBe(1)
    expect(arrivals).toBe(0)
    expect(retry).toBeNull()
    expect(calls).toHaveLength(1)
  })

  it('frees the in-flight slot after a rejection so other tiles keep loading', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, 5 * TILE_COLS, 1, 0)

    calls[0].reject(new Error('io'))
    await flush()

    expect(calls).toHaveLength(5)
  })

  it('treats a fetcher that throws synchronously as a failed tile', async () => {
    let attempts = 0
    const throwingFetcher: WaveFetcher = () => {
      attempts += 1
      throw new Error('boom')
    }
    const cache = new WaveformCache(throwingFetcher, () => {})

    expect(cache.range(1, 0, 100, 1, 0)).toBeNull()
    await flush()
    cache.range(1, 0, 100, 1, 0)

    expect(cache.failedTileCount).toBe(1)
    expect(attempts).toBe(1)
  })
})

describe('WaveformCache epochs', () => {
  it('discards a result that arrives after reset', async () => {
    const { calls, fetcher } = createFakeFetcher()
    let arrivals = 0
    const cache = new WaveformCache(fetcher, () => {
      arrivals += 1
    })
    cache.range(1, 0, 100, 1, 0)
    cache.reset(2)

    calls[0].resolve(uniformTile(0.5))
    await flush()

    expect(arrivals).toBe(0)
    expect(cache.range(1, 0, 100, 1, 0)).toBeNull()
  })

  it('requests the tile again after reset and uses the new result', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, 100, 1, 0)
    cache.reset(2)

    cache.range(1, 0, 100, 1, 0)
    calls[1].resolve(uniformTile(0.3))
    await flush()

    expect(calls).toHaveLength(2)
    expect(cache.range(1, 0, 100, 1, 0)).toEqual([-0.3, 0.3])
  })

  it('counts stale fetches that are still outstanding against the in-flight cap', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, MAX_IN_FLIGHT * TILE_COLS, 1, 0)
    cache.reset(2)

    cache.range(2, 0, 100, 1, 0)
    expect(calls).toHaveLength(MAX_IN_FLIGHT)

    calls[0].resolve(uniformTile(0.1))
    await flush()
    expect(calls).toHaveLength(MAX_IN_FLIGHT + 1)
  })

  it('clears loaded tiles on reset', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    cache.range(1, 0, 100, 1, 0)
    calls[0].resolve(uniformTile(0.5))
    await flush()

    cache.reset(2)

    expect(cache.range(1, 0, 100, 1, 0)).toBeNull()
    expect(cache.failedTileCount).toBe(0)
  })
})

describe('WaveformCache LRU', () => {
  it('evicts the least recently used tile once the cap is exceeded', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const columns = uniformTile(0.1)

    for (let index = 0; index <= LRU_CAP; index++) {
      cache.range(1, index * TILE_COLS, index * TILE_COLS + 1, 1, index * TILE_COLS)
      await settleAll(calls, columns)
    }

    expect(cache.range(1, 0, 1, 1, 0)).toBeNull()
    expect(cache.range(1, TILE_COLS, TILE_COLS + 1, 1, 0)).not.toBeNull()
    expect(cache.range(1, LRU_CAP * TILE_COLS, LRU_CAP * TILE_COLS + 1, 1, 0)).not.toBeNull()
  })

  it('keeps a recently used tile alive while older ones are evicted', async () => {
    const { calls, fetcher } = createFakeFetcher()
    const cache = new WaveformCache(fetcher, () => {})
    const columns = uniformTile(0.1)

    for (let index = 0; index <= LRU_CAP; index++) {
      cache.range(1, index * TILE_COLS, index * TILE_COLS + 1, 1, index * TILE_COLS)
      cache.range(1, 0, 1, 1, 0)
      await settleAll(calls, columns)
    }

    expect(cache.range(1, 0, 1, 1, 0)).not.toBeNull()
    expect(cache.range(1, TILE_COLS, TILE_COLS + 1, 1, 0)).toBeNull()
  })
})
