import { describe, expect, it, vi } from 'vitest'

vi.mock('./store.svelte', () => ({
  store: {
    status: { master: { peakL: 0.5, peakR: 0.25, rmsL: 0.1, rmsR: 0.1 } },
    levelOf: (id: number) => (id === 1 ? { peakL: 1, peakR: 1, rmsL: 1, rmsR: 1 } : undefined),
  },
}))

import { createMeterBank, MAX_FRAME_DT_S, type MeterFrame, type MeterKey, type MeterSink } from './meterBank'
import type { StereoLevel } from './types'

/** Manual scheduler: `flush(ts)` runs the pending callback like a display frame would. */
function fakeScheduler(): {
  requestFrame: (cb: (ts: number) => void) => number
  cancelFrame: (handle: number) => void
  flush: (ts: number) => void
  pending: () => number
  requested: () => number
  cancelled: () => number
} {
  let nextHandle = 1
  let callback: ((ts: number) => void) | null = null
  let handle: number | null = null
  let requested = 0
  let cancelled = 0
  return {
    requestFrame(cb) {
      requested++
      callback = cb
      handle = nextHandle++
      return handle
    },
    cancelFrame(h) {
      cancelled++
      if (h === handle) {
        callback = null
        handle = null
      }
    },
    flush(ts) {
      const cb = callback
      callback = null
      handle = null
      cb?.(ts)
    },
    pending: () => (callback ? 1 : 0),
    requested: () => requested,
    cancelled: () => cancelled,
  }
}

function recordingSink(key: MeterKey): MeterSink & { frames: MeterFrame[] } {
  const frames: MeterFrame[] = []
  return {
    key,
    frames,
    frame(f: MeterFrame) {
      frames.push(f)
    },
  }
}

const LEVEL: StereoLevel = { peakL: 0.3, peakR: 0.2, rmsL: 0.1, rmsR: 0.1 }

function makeBank(levels: Record<string, StereoLevel | undefined> = {}) {
  const sched = fakeScheduler()
  const bank = createMeterBank({
    readLevel: (key) => levels[String(key)],
    requestFrame: sched.requestFrame,
    cancelFrame: sched.cancelFrame,
  })
  return { bank, sched }
}

describe('meterBank registration', () => {
  it('starts with no sinks and the loop idle', () => {
    const { bank } = makeBank()
    expect(bank.size).toBe(0)
    expect(bank.running).toBe(false)
  })

  it('starts the loop when the first sink registers', () => {
    const { bank, sched } = makeBank()
    bank.register(recordingSink('master'))
    expect(bank.running).toBe(true)
    expect(sched.requested()).toBe(1)
  })

  it('shares one loop across many sinks instead of requesting one per sink', () => {
    const { bank, sched } = makeBank()
    bank.register(recordingSink('master'))
    bank.register(recordingSink(1))
    bank.register(recordingSink(2))
    expect(bank.size).toBe(3)
    expect(sched.requested()).toBe(1)
  })

  it('stops and cancels the pending frame when the last sink unregisters', () => {
    const { bank, sched } = makeBank()
    const off = bank.register(recordingSink('master'))
    off()
    expect(bank.size).toBe(0)
    expect(bank.running).toBe(false)
    expect(sched.cancelled()).toBe(1)
    expect(sched.pending()).toBe(0)
  })

  it('keeps the loop running while any sink remains', () => {
    const { bank, sched } = makeBank()
    const offA = bank.register(recordingSink('master'))
    bank.register(recordingSink(1))
    offA()
    expect(bank.running).toBe(true)
    expect(sched.cancelled()).toBe(0)
  })

  it('does not stop twice when unregister is called twice', () => {
    const { bank, sched } = makeBank()
    const off = bank.register(recordingSink('master'))
    off()
    off()
    expect(sched.cancelled()).toBe(1)
  })
})

describe('meterBank frames', () => {
  it('feeds the master level to a master sink', () => {
    const master: StereoLevel = { peakL: 0.5, peakR: 0.25, rmsL: 0.1, rmsR: 0.1 }
    const { bank, sched } = makeBank({ master })
    const sink = recordingSink('master')
    bank.register(sink)
    sched.flush(1000)
    expect(sink.frames[0].level).toBe(master)
  })

  it('feeds a track level to the sink with that track id', () => {
    const { bank, sched } = makeBank({ '7': LEVEL })
    const sink = recordingSink(7)
    bank.register(sink)
    sched.flush(1000)
    expect(sink.frames[0].level).toBe(LEVEL)
  })

  it('passes undefined for a track that has no level, such as a removed track', () => {
    const { bank, sched } = makeBank({})
    const sink = recordingSink(99)
    bank.register(sink)
    sched.flush(1000)
    expect(sink.frames[0].level).toBeUndefined()
  })

  it('reports dt 0 on the first frame of a run', () => {
    const { bank, sched } = makeBank({})
    const sink = recordingSink('master')
    bank.register(sink)
    sched.flush(5000)
    expect(sink.frames[0].dtSeconds).toBe(0)
    expect(sink.frames[0].nowMs).toBe(5000)
  })

  it('reports the seconds elapsed between frames', () => {
    const { bank, sched } = makeBank({})
    const sink = recordingSink('master')
    bank.register(sink)
    sched.flush(1000)
    sched.flush(1016.6)
    expect(sink.frames[1].dtSeconds).toBeCloseTo(0.0166, 4)
  })

  it('clamps a large gap between frames so ballistics cannot jump', () => {
    const { bank, sched } = makeBank({})
    const sink = recordingSink('master')
    bank.register(sink)
    sched.flush(0)
    sched.flush(10_000)
    expect(sink.frames[1].dtSeconds).toBe(MAX_FRAME_DT_S)
  })

  it('reads each sink once per frame and keeps the loop scheduled', () => {
    const { bank, sched } = makeBank({})
    const a = recordingSink('master')
    const b = recordingSink(1)
    bank.register(a)
    bank.register(b)
    sched.flush(0)
    sched.flush(16)
    expect(a.frames).toHaveLength(2)
    expect(b.frames).toHaveLength(2)
    expect(sched.pending()).toBe(1)
  })

  it('stops scheduling frames once the last sink unregisters from inside a frame', () => {
    const { bank, sched } = makeBank({})
    let off: () => void = () => undefined
    const sink: MeterSink = {
      key: 'master',
      frame() {
        off()
      },
    }
    off = bank.register(sink)
    sched.flush(0)
    expect(bank.running).toBe(false)
    expect(sched.pending()).toBe(0)
  })

  it('restarts with dt 0 after the loop was stopped and started again', () => {
    const { bank, sched } = makeBank({})
    const first = recordingSink('master')
    const off = bank.register(first)
    sched.flush(0)
    sched.flush(16)
    off()
    const second = recordingSink('master')
    bank.register(second)
    sched.flush(10_000)
    expect(second.frames[0].dtSeconds).toBe(0)
  })
})

describe('meterBank with the app store', () => {
  it('uses the store-backed default bank without throwing when no sink is registered', async () => {
    const { meterBank } = await import('./meterBank')
    expect(meterBank.size).toBe(0)
    expect(meterBank.running).toBe(false)
  })
})
