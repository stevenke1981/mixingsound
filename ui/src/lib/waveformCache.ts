// Waveform tile cache (ui-spec §5.3). STUB — WP-B1 implements.
import type { WaveformColumns } from './types'

export type WaveFetcher = (sourceId: number, start: number, end: number, columns: number) => Promise<WaveformColumns>

export const TILE_COLS = 512
export const MAX_IN_FLIGHT = 4
export const LRU_CAP = 400

export function quantizeFpp(fpp: number): number {
  return Math.max(1, Math.round(fpp))
}

export class WaveformCache {
  constructor(fetcher: WaveFetcher, onTile: () => void) {}
  /** Drop everything (new/open project). */
  reset(epoch: number): void {}
  /**
   * Min/max over source frames `[from, to)` at zoom `fpp`, or null when no tile is loaded yet.
   * Missing tiles are requested (nearest `centerFrame` first, ≤ MAX_IN_FLIGHT, no duplicates);
   * `onTile` fires on arrival.
   */
  range(sourceId: number, from: number, to: number, fpp: number, centerFrame: number): [number, number] | null {
    return null
  }
}
