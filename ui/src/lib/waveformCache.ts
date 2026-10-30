// Waveform tile cache (ui-spec §5.3).
import type { WaveformColumns } from './types'

export type WaveFetcher = (sourceId: number, start: number, end: number, columns: number) => Promise<WaveformColumns>

export const TILE_COLS = 512
export const MAX_IN_FLIGHT = 4
export const LRU_CAP = 400

/**
 * Bound on queued-but-not-started tiles. `range()` re-adds every visible missing tile on each
 * call, so dropping an overflow is safe: it is simply requested again on the next draw.
 */
const PENDING_CAP = 4 * LRU_CAP

/**
 * Zoom quantised to 4 steps per octave: `k = round(4·log2(fpp))`, `fppQ = max(1, round(2^(k/4)))`.
 * Non-finite or non-positive input falls back to 1 (finest zoom) rather than producing NaN keys.
 */
export function quantizeFpp(fpp: number): number {
  if (!Number.isFinite(fpp) || fpp <= 0) return 1
  const k = Math.round(4 * Math.log2(fpp))
  return Math.max(1, Math.round(2 ** (k / 4)))
}

interface PendingTile {
  key: string
  sourceId: number
  index: number
  fppQ: number
  distance: number
}

/**
 * Min/max peak tiles keyed by `epoch:sourceId:fppQ:index`.
 *
 * Failure policy: a fetch error drops the tile from in-flight and marks it failed for the current
 * generation; it is not retried until `reset()`. Results from before the last `reset()` are discarded.
 * `MAX_IN_FLIGHT` counts every fetcher call still outstanding, including stale ones from before a reset.
 */
export class WaveformCache {
  readonly #fetcher: WaveFetcher
  readonly #onTile: () => void
  #epoch = 0
  /** Bumped by every `reset()`; a fetch result is accepted only if its generation still matches. */
  #generation = 0
  /** Insertion order is recency order (oldest first). */
  readonly #loaded = new Map<string, WaveformColumns>()
  readonly #inFlight = new Set<string>()
  readonly #failed = new Set<string>()
  readonly #pending = new Map<string, PendingTile>()
  #outstanding = 0

  constructor(fetcher: WaveFetcher, onTile: () => void) {
    this.#fetcher = fetcher
    this.#onTile = onTile
  }

  /** Tiles whose fetch failed in the current generation. */
  get failedTileCount(): number {
    return this.#failed.size
  }

  /** Drop everything (new/open project). */
  reset(epoch: number): void {
    this.#epoch = epoch
    this.#generation += 1
    this.#loaded.clear()
    this.#inFlight.clear()
    this.#failed.clear()
    this.#pending.clear()
  }

  /**
   * Min/max over source frames `[from, to)` at zoom `fpp`, or null when no tile is loaded yet.
   * Missing tiles are requested (nearest `centerFrame` first, ≤ MAX_IN_FLIGHT, no duplicates);
   * `onTile` fires on arrival.
   *
   * Partial result: if only some covering tiles are loaded, the min/max covers the loaded columns only.
   * Null means no covering column is loaded (or the range is empty).
   * Intended for pixel-sized ranges; a range spanning many tiles costs one loop step per tile.
   */
  range(sourceId: number, from: number, to: number, fpp: number, centerFrame: number): [number, number] | null {
    const start = Math.max(0, from)
    if (!(to > start)) return null

    const fppQ = quantizeFpp(fpp)
    const span = TILE_COLS * fppQ
    const firstTile = Math.floor(start / span)
    const lastTile = Math.floor((to - 1) / span)

    let min = Infinity
    let max = -Infinity
    for (let index = firstTile; index <= lastTile; index++) {
      const key = this.#key(sourceId, fppQ, index)
      const columns = this.#touch(key)
      if (columns === undefined) {
        this.#want(key, sourceId, fppQ, index, centerFrame)
        continue
      }
      const peaks = aggregateTile(columns, index * span, start, to, fppQ)
      if (peaks === null) continue
      min = Math.min(min, peaks[0])
      max = Math.max(max, peaks[1])
    }

    this.#pump()
    return min === Infinity ? null : [min, max]
  }

  #key(sourceId: number, fppQ: number, index: number): string {
    return `${this.#epoch}:${sourceId}:${fppQ}:${index}`
  }

  /** Returns the loaded tile and marks it most recently used. */
  #touch(key: string): WaveformColumns | undefined {
    const columns = this.#loaded.get(key)
    if (columns !== undefined) {
      this.#loaded.delete(key)
      this.#loaded.set(key, columns)
    }
    return columns
  }

  #want(key: string, sourceId: number, fppQ: number, index: number, centerFrame: number): void {
    if (this.#inFlight.has(key) || this.#failed.has(key)) return
    if (!this.#pending.has(key) && this.#pending.size >= PENDING_CAP) return
    const tileCentre = (index * TILE_COLS + TILE_COLS / 2) * fppQ
    this.#pending.set(key, {
      key,
      sourceId,
      index,
      fppQ,
      distance: Math.abs(tileCentre - centerFrame),
    })
  }

  /** Starts queued fetches until the in-flight cap is reached or nothing is queued. */
  #pump(): void {
    while (this.#outstanding < MAX_IN_FLIGHT) {
      const next = this.#takeNearest()
      if (next === null) return
      if (this.#loaded.has(next.key) || this.#inFlight.has(next.key) || this.#failed.has(next.key)) continue
      this.#start(next)
    }
  }

  #takeNearest(): PendingTile | null {
    let best: PendingTile | null = null
    for (const entry of this.#pending.values()) {
      if (best === null || entry.distance < best.distance) best = entry
    }
    if (best !== null) this.#pending.delete(best.key)
    return best
  }

  #start(tile: PendingTile): void {
    const { key, sourceId, index, fppQ } = tile
    const start = index * TILE_COLS * fppQ
    const end = start + TILE_COLS * fppQ
    const generation = this.#generation

    this.#inFlight.add(key)
    this.#outstanding += 1

    let request: Promise<WaveformColumns>
    try {
      request = Promise.resolve(this.#fetcher(sourceId, start, end, TILE_COLS))
    } catch (error) {
      request = Promise.reject(error)
    }
    request.then(
      (columns) => this.#settle(generation, key, columns),
      () => this.#settle(generation, key, null),
    )
  }

  /** `columns === null` means the fetch failed. Stale generations only release their slot. */
  #settle(generation: number, key: string, columns: WaveformColumns | null): void {
    this.#outstanding -= 1
    if (generation === this.#generation) {
      this.#inFlight.delete(key)
      if (columns === null) {
        this.#failed.add(key)
      } else {
        this.#store(key, columns)
        this.#onTile()
      }
    }
    this.#pump()
  }

  #store(key: string, columns: WaveformColumns): void {
    this.#loaded.delete(key)
    this.#loaded.set(key, columns)
    while (this.#loaded.size > LRU_CAP) {
      const oldest = this.#loaded.keys().next().value
      if (oldest === undefined) return
      this.#loaded.delete(oldest)
    }
  }
}

/**
 * Min/max over the tile columns that overlap source frames `[from, to)`.
 * Column `c` covers `fppQ` frames starting at `tileStart + c·fppQ`. Tiles that returned a
 * different column count are read proportionally, so any non-empty array is usable.
 */
function aggregateTile(
  columns: WaveformColumns,
  tileStart: number,
  from: number,
  to: number,
  fppQ: number,
): [number, number] | null {
  const count = columns.length
  if (count === 0) return null
  const firstColumn = Math.max(0, Math.floor((from - tileStart) / fppQ))
  const lastColumn = Math.min(TILE_COLS - 1, Math.floor((to - 1 - tileStart) / fppQ))
  if (lastColumn < firstColumn) return null

  let min = Infinity
  let max = -Infinity
  for (let column = firstColumn; column <= lastColumn; column++) {
    const index = count === TILE_COLS ? column : Math.min(count - 1, Math.floor((column * count) / TILE_COLS))
    const peak = columns[index]
    min = Math.min(min, peak[0])
    max = Math.max(max, peak[1])
  }
  return min === Infinity ? null : [min, max]
}
