// One shared animation-frame loop for every Meter on screen (ui-spec §6.3 WP-C).
// Meters register a sink; the loop runs only while at least one is registered and reads levels
// from the store (master + tracks by id) once per frame. Scheduling is injectable for tests.

import { store } from './store.svelte'
import type { StereoLevel } from './types'

/** Master meter key, or the track id. */
export type MeterKey = number | 'master'

export interface MeterFrame {
  /** Latest engine level for this key; undefined when the track no longer exists. */
  level: StereoLevel | undefined
  /** Seconds since the previous frame, clamped to MAX_FRAME_DT_S. 0 on the first frame of a run. */
  dtSeconds: number
  /** rAF timestamp, milliseconds. */
  nowMs: number
}

export interface MeterSink {
  readonly key: MeterKey
  frame(frame: MeterFrame): void
}

export interface MeterBankDeps {
  readLevel(key: MeterKey): StereoLevel | undefined
  requestFrame(callback: (timestampMs: number) => void): number
  cancelFrame(handle: number): void
}

export interface MeterBank {
  /** Adds a sink; returns the unregister function. Starts the loop when it was idle. */
  register(sink: MeterSink): () => void
  /** Number of registered sinks. */
  readonly size: number
  /** True while the animation-frame loop is scheduled. */
  readonly running: boolean
}

/** A stalled tab can return with a huge gap; clamping keeps ballistics from jumping. */
export const MAX_FRAME_DT_S = 0.1

export function createMeterBank(deps: MeterBankDeps): MeterBank {
  const sinks = new Set<MeterSink>()
  let handle: number | null = null
  let lastTimestampMs: number | null = null

  function scheduleNext(): void {
    if (sinks.size === 0) {
      lastTimestampMs = null
      return
    }
    if (handle === null) handle = deps.requestFrame(tick)
  }

  function tick(timestampMs: number): void {
    handle = null
    const dtSeconds =
      lastTimestampMs === null
        ? 0
        : Math.min(MAX_FRAME_DT_S, Math.max(0, (timestampMs - lastTimestampMs) / 1000))
    lastTimestampMs = timestampMs
    for (const sink of sinks) {
      sink.frame({ level: deps.readLevel(sink.key), dtSeconds, nowMs: timestampMs })
    }
    scheduleNext()
  }

  return {
    register(sink: MeterSink): () => void {
      sinks.add(sink)
      scheduleNext()
      return () => {
        sinks.delete(sink)
        if (sinks.size === 0 && handle !== null) {
          deps.cancelFrame(handle)
          handle = null
          lastTimestampMs = null
        }
      }
    },
    get size(): number {
      return sinks.size
    },
    get running(): boolean {
      return handle !== null
    },
  }
}

/** The app-wide bank, fed from the store. */
export const meterBank: MeterBank = createMeterBank({
  readLevel: (key) => (key === 'master' ? store.status.master : store.levelOf(key)),
  requestFrame: (callback) => requestAnimationFrame(callback),
  cancelFrame: (handle) => cancelAnimationFrame(handle),
})
