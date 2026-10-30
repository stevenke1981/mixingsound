import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { api, onEngineStatus, onExportProgress } from './api'
import { pickExportPath, pickProjectFile, pickProjectSavePath, subscribeFileDrop } from './os'
import { AppStore } from './store.svelte'
import type { AppSnapshot, BounceReport, Edit, EngineStatus, Project, StereoLevel } from './types'

vi.mock('./api', () => ({
  api: {
    init: vi.fn(),
    newProject: vi.fn(),
    importFiles: vi.fn(),
    applyEdit: vi.fn(),
    endGesture: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    play: vi.fn(),
    pause: vi.fn(),
    stop: vi.fn(),
    seek: vi.fn(),
    setLoop: vi.fn(),
    saveProject: vi.fn(),
    openProject: vi.fn(),
    exportWav: vi.fn(),
  },
  onEngineStatus: vi.fn(),
  onExportProgress: vi.fn(),
  errorMessage: (e: unknown): string => {
    if (typeof e === 'string') return e
    if (e instanceof Error) return e.message
    return 'Unexpected error'
  },
}))

vi.mock('./os', async (importOriginal) => ({
  ...(await importOriginal<typeof import('./os')>()),
  pickAudioFiles: vi.fn(),
  pickProjectFile: vi.fn(),
  pickProjectSavePath: vi.fn(),
  pickExportPath: vi.fn(),
  subscribeFileDrop: vi.fn(),
}))

const SR = 48000
const LEVEL: StereoLevel = { peakL: 0, peakR: 0, rmsL: 0, rmsR: 0 }

interface Deferred<T> {
  promise: Promise<T>
  resolve: (value: T) => void
  reject: (reason: unknown) => void
}

function deferred<T>(): Deferred<T> {
  let resolve: (value: T) => void = () => undefined
  let reject: (reason: unknown) => void = () => undefined
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/** Lets pending promise continuations run under fake timers. */
async function flush(): Promise<void> {
  for (let i = 0; i < 10; i++) await Promise.resolve()
}

function project(clipLength = 4 * SR): Project {
  return {
    version: 1,
    sampleRate: SR,
    tracks: [
      {
        id: 1,
        name: 'Vocals',
        gainDb: 0,
        pan: 0,
        mute: false,
        solo: false,
        color: '#d98c3f',
        clips: [
          { id: 10, sourceId: 1, start: 0, offset: 0, length: clipLength, gainDb: 0, fadeIn: 0, fadeOut: 0 },
        ],
      },
    ],
    master: { gainDb: 0 },
    sources: [{ id: 1, name: 'vox.wav', path: 'C:/x/vox.wav', channels: 2, frames: 10 * SR, originalSampleRate: SR }],
    nextId: 20,
  }
}

function snapshot(overrides: Partial<AppSnapshot> = {}): AppSnapshot {
  return {
    project: project(),
    canUndo: false,
    canRedo: false,
    projectPath: null,
    dirty: false,
    deviceName: 'Speakers',
    missingSources: [],
    warnings: [],
    ...overrides,
  }
}

function status(overrides: Partial<EngineStatus> = {}): EngineStatus {
  return { playing: false, position: 0, master: LEVEL, tracks: [], clipped: false, error: null, ...overrides }
}

function level(peak: number): StereoLevel {
  return { peakL: peak, peakR: 0, rmsL: 0, rmsR: 0 }
}

const REPORT: BounceReport = { path: 'C:/out/mix.wav', frames: 4 * SR, peakDb: -1.8, clippedSamples: 0 }

let store: AppStore
let emitStatus: (s: EngineStatus) => void
let emitProgress: (f: number) => void

async function started(snap: AppSnapshot = snapshot()): Promise<AppStore> {
  vi.mocked(api.init).mockResolvedValue(snap)
  await store.init()
  return store
}

beforeEach(() => {
  vi.useFakeTimers()
  vi.setSystemTime(new Date('2026-10-30T00:00:00Z'))
  vi.clearAllMocks()
  vi.mocked(onEngineStatus).mockImplementation(async (cb) => {
    emitStatus = cb
    return () => undefined
  })
  vi.mocked(onExportProgress).mockImplementation(async (cb) => {
    emitProgress = cb
    return () => undefined
  })
  vi.mocked(subscribeFileDrop).mockResolvedValue(() => undefined)
  vi.mocked(api.applyEdit).mockResolvedValue(snapshot())
  vi.mocked(api.play).mockResolvedValue(undefined)
  vi.mocked(api.pause).mockResolvedValue(undefined)
  vi.mocked(api.seek).mockResolvedValue(undefined)
  vi.mocked(api.setLoop).mockResolvedValue(undefined)
  vi.mocked(api.endGesture).mockResolvedValue(undefined)
  store = new AppStore()
})

afterEach(() => {
  store.dispose()
  vi.useRealTimers()
})

describe('init', () => {
  it('loads the snapshot and sets the default zoom from the sample rate', async () => {
    // Arrange / Act
    await started()
    // Assert
    expect(store.project?.tracks).toHaveLength(1)
    expect(store.view.fpp).toBe(SR / 100)
  })

  it('turns an init failure into an error toast instead of throwing', async () => {
    vi.mocked(api.init).mockRejectedValue('engine missing')
    await expect(store.init()).resolves.toBeUndefined()
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'error', text: 'engine missing' })
  })
})

describe('transport', () => {
  it('play enables follow and calls the engine when a device is present', async () => {
    await started()
    store.toggleFollow()
    await store.play()
    expect(api.play).toHaveBeenCalledOnce()
    expect(store.view.follow).toBe(true)
  })

  it('play warns and does not call the engine without an audio device', async () => {
    await started(snapshot({ deviceName: null }))
    await store.play()
    expect(api.play).not.toHaveBeenCalled()
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'warn', text: 'No audio output device' })
  })

  it('seek moves the playhead before the engine answers', async () => {
    await started()
    const reply = deferred<void>()
    vi.mocked(api.seek).mockReturnValue(reply.promise)
    const pending = store.seek(1000)
    expect(store.position).toBe(1000)
    reply.resolve()
    await pending
    expect(api.seek).toHaveBeenCalledWith(1000)
  })

  it('ignores engine position during the seek hold, then follows it again', async () => {
    await started()
    await store.seek(1000)
    emitStatus(status({ position: 0 }))
    expect(store.position).toBe(1000)
    vi.advanceTimersByTime(301)
    emitStatus(status({ position: 2000 }))
    expect(store.position).toBe(2000)
  })

  it('a failing play becomes an error toast and never throws', async () => {
    await started()
    vi.mocked(api.play).mockRejectedValue('device lost')
    await expect(store.play()).resolves.toBeUndefined()
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'error', text: 'device lost' })
  })

  it('setLoopRegion enables the loop and rejects an empty region', async () => {
    await started()
    await store.setLoopRegion(100, 100)
    expect(api.setLoop).not.toHaveBeenCalled()
    await store.setLoopRegion(100, 9600)
    expect(store.loop).toEqual({ start: 100, end: 9600 })
    expect(store.loopEnabled).toBe(true)
  })
})

describe('live edits', () => {
  it('sends the first value at once, then only the newest value, one request in flight', async () => {
    // Arrange
    await started()
    const replies: Deferred<AppSnapshot>[] = []
    vi.mocked(api.applyEdit).mockImplementation(() => {
      const reply = deferred<AppSnapshot>()
      replies.push(reply)
      return reply.promise
    })
    // Act
    store.setTrackGain(1, -10)
    store.setTrackGain(1, -8)
    store.setTrackGain(1, -6)
    // Assert: only the first has gone out
    expect(api.applyEdit).toHaveBeenCalledTimes(1)
    expect(vi.mocked(api.applyEdit).mock.calls[0]?.[0]).toMatchObject({ gainDb: -10 })
    replies[0]?.resolve(snapshot())
    await flush()
    // The intermediate -8 is skipped; the newest value (-6) is sent next
    expect(api.applyEdit).toHaveBeenCalledTimes(2)
    expect(vi.mocked(api.applyEdit).mock.calls[1]?.[0]).toMatchObject({ gainDb: -6 })
  })

  it('keeps separate keys independent', async () => {
    await started()
    vi.mocked(api.applyEdit).mockReturnValue(new Promise(() => undefined))
    store.setTrackGain(1, -3)
    store.setTrackPan(1, 0.5)
    expect(api.applyEdit).toHaveBeenCalledTimes(2)
  })

  it('endGesture flushes the pending value before calling the engine', async () => {
    await started()
    const replies: Deferred<AppSnapshot>[] = []
    vi.mocked(api.applyEdit).mockImplementation(() => {
      const reply = deferred<AppSnapshot>()
      replies.push(reply)
      return reply.promise
    })
    store.setTrackGain(1, -10)
    store.setTrackGain(1, -4)
    const ending = store.endGesture()
    replies[0]?.resolve(snapshot())
    await flush()
    expect(api.endGesture).not.toHaveBeenCalled()
    replies[1]?.resolve(snapshot())
    await ending
    const lastEdit = vi.mocked(api.applyEdit).mock.invocationCallOrder[1] ?? 0
    expect(vi.mocked(api.endGesture).mock.invocationCallOrder[0]).toBeGreaterThan(lastEdit)
    expect(api.endGesture).toHaveBeenCalledOnce()
  })

  it('a failing live edit toasts the error and the next value still goes out', async () => {
    await started()
    vi.mocked(api.applyEdit).mockRejectedValueOnce('gain rejected')
    store.setTrackGain(1, -2)
    await flush()
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'error', text: 'gain rejected' })
    vi.mocked(api.applyEdit).mockResolvedValue(snapshot())
    store.setTrackGain(1, -3)
    await flush()
    expect(api.applyEdit).toHaveBeenCalledTimes(2)
  })
})

describe('snapshots, warnings and errors', () => {
  it('only warnings that are new in a snapshot become warn toasts', async () => {
    await started(snapshot({ warnings: ['resampled'] }))
    vi.mocked(api.applyEdit).mockResolvedValue(snapshot({ warnings: ['resampled', 'two files skipped'] }))
    await store.edit({ type: 'setTrackMute', track: 1, mute: true })
    const warnTexts = store.toasts.filter((t) => t.kind === 'warn').map((t) => t.text)
    expect(warnTexts).toEqual(['resampled', 'two files skipped'])
  })

  it('a failed edit toasts the error, returns false and keeps the old snapshot', async () => {
    await started()
    const before = store.snapshot
    vi.mocked(api.applyEdit).mockRejectedValue('Track is locked')
    const ok = await store.edit({ type: 'setTrackMute', track: 1, mute: true })
    expect(ok).toBe(false)
    expect(store.snapshot).toBe(before)
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'error', text: 'Track is locked' })
  })

  it('an engine status error is toasted once per distinct error', async () => {
    await started()
    emitStatus(status({ error: 'stream xrun' }))
    emitStatus(status({ error: 'stream xrun' }))
    emitStatus(status({ error: 'stream xrun' }))
    expect(store.toasts.filter((t) => t.kind === 'error')).toHaveLength(1)
    emitStatus(status({ error: null }))
    // Identical toasts inside the two-second dedupe window are suppressed; wait it out.
    vi.advanceTimersByTime(2001)
    emitStatus(status({ error: 'stream xrun' }))
    expect(store.toasts.filter((t) => t.kind === 'error')).toHaveLength(2)
  })
})

describe('clip latch', () => {
  it('latches a track clip LED on full-scale peak and keeps it until cleared', async () => {
    await started()
    emitStatus(status({ tracks: [{ id: 1, level: level(1) }] }))
    expect(store.clipLatch[1]).toBe(true)
    emitStatus(status({ tracks: [{ id: 1, level: level(0.1) }] }))
    expect(store.clipLatch[1]).toBe(true)
    store.clearClip(1)
    expect(store.clipLatch[1]).toBe(false)
  })

  it('latches the master from the engine clipped flag', async () => {
    await started()
    emitStatus(status({ clipped: true }))
    expect(store.clipLatch.master).toBe(true)
  })

  it('clears every latch when a new project is created', async () => {
    await started()
    emitStatus(status({ clipped: true, tracks: [{ id: 1, level: level(1) }] }))
    vi.mocked(api.newProject).mockResolvedValue(snapshot())
    await store.newProject()
    expect(store.clipLatch).toEqual({})
  })
})

describe('follow-scroll', () => {
  beforeEach(async () => {
    await started()
    store.setViewport(1000, 600)
    store.view.fpp = 100
  })

  it('scrolls so the playhead stays in view while playing with follow on', () => {
    // 1000 px at 100 frames/px: the view spans 100 000 frames, the follow edge is at 90 000
    emitStatus(status({ playing: true, position: 95000 }))
    expect(store.view.scrollFrames).toBe(85000)
  })

  it('does not scroll while paused', () => {
    emitStatus(status({ playing: false, position: 95000 }))
    expect(store.view.scrollFrames).toBe(0)
  })

  it('a manual scroll during playback turns follow off until the next play', async () => {
    emitStatus(status({ playing: true, position: 10 }))
    store.scrollByPx(10)
    expect(store.view.follow).toBe(false)
    emitStatus(status({ playing: true, position: 95000 }))
    expect(store.view.scrollFrames).toBe(1000)
    await store.play()
    expect(store.view.follow).toBe(true)
  })
})

describe('export', () => {
  it('runs form, running and done, with progress and the engine report', async () => {
    // Arrange
    await started()
    vi.mocked(pickExportPath).mockResolvedValue('C:/out/mix.wav')
    const reply = deferred<BounceReport>()
    vi.mocked(api.exportWav).mockReturnValue(reply.promise)
    store.openExport()
    // Act
    const running = store.runExport({ range: 'project', bitDepth: 'int16', dither: true })
    await flush()
    // Assert: running
    expect(store.export.phase).toBe('running')
    expect(store.busy).toBe('export')
    emitProgress(0.4)
    expect(store.export.progress).toBe(0.4)
    reply.resolve(REPORT)
    await running
    // Assert: done
    expect(store.export.phase).toBe('done')
    expect(store.export.report).toEqual(REPORT)
    expect(store.busy).toBeNull()
    expect(api.exportWav).toHaveBeenCalledWith('C:/out/mix.wav', {
      start: 0,
      end: 4 * SR,
      bitDepth: 'int16',
      dither: true,
    })
  })

  it('an export failure moves to the error phase and raises an error toast', async () => {
    await started()
    vi.mocked(pickExportPath).mockResolvedValue('C:/out/mix.wav')
    vi.mocked(api.exportWav).mockRejectedValue('disk full')
    store.openExport()
    await store.runExport({ range: 'project', bitDepth: 'float32', dither: false })
    expect(store.export.phase).toBe('error')
    expect(store.export.error).toBe('disk full')
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'error', text: 'disk full' })
  })

  it('cancelling the save dialog keeps the form open and exports nothing', async () => {
    await started()
    vi.mocked(pickExportPath).mockResolvedValue(null)
    store.openExport()
    await store.runExport({ range: 'project', bitDepth: 'int24', dither: false })
    expect(store.export.phase).toBe('form')
    expect(api.exportWav).not.toHaveBeenCalled()
  })

  it('a loop export without a loop region fails before the save dialog opens', async () => {
    await started()
    store.openExport()
    await store.runExport({ range: 'loop', bitDepth: 'int24', dither: false })
    expect(store.export.phase).toBe('error')
    expect(pickExportPath).not.toHaveBeenCalled()
  })

  it('the export dialog cannot be closed while running', async () => {
    await started()
    vi.mocked(pickExportPath).mockResolvedValue('C:/out/mix.wav')
    vi.mocked(api.exportWav).mockReturnValue(new Promise(() => undefined))
    store.openExport()
    void store.runExport({ range: 'project', bitDepth: 'int24', dither: false })
    await flush()
    store.closeExport()
    expect(store.export.open).toBe(true)
  })
})

describe('project files and the dirty guard', () => {
  it('opening a project with missing sources raises exactly one warning', async () => {
    await started()
    vi.mocked(pickProjectFile).mockResolvedValue('C:/p/a.msproj')
    vi.mocked(api.openProject).mockResolvedValue(snapshot({ missingSources: [1], projectPath: 'C:/p/a.msproj' }))
    const epochBefore = store.projectEpoch
    await store.openDialog()
    const warns = store.toasts.filter((t) => t.kind === 'warn')
    expect(warns).toHaveLength(1)
    expect(store.projectEpoch).toBe(epochBefore + 1)
  })

  it('opening a dirty project waits for the choice and does nothing on cancel', async () => {
    await started(snapshot({ dirty: true }))
    const opening = store.openPath('C:/p/other.msproj')
    await flush()
    expect(store.confirmRequest).not.toBeNull()
    store.resolveConfirm('cancel')
    await opening
    expect(api.openProject).not.toHaveBeenCalled()
  })

  it('choosing discard continues the open', async () => {
    await started(snapshot({ dirty: true }))
    vi.mocked(api.openProject).mockResolvedValue(snapshot())
    const opening = store.openPath('C:/p/other.msproj')
    await flush()
    store.resolveConfirm('discard')
    await opening
    expect(api.openProject).toHaveBeenCalledWith('C:/p/other.msproj')
  })

  it('save as uses the save dialog and writes to the chosen path', async () => {
    await started()
    vi.mocked(pickProjectSavePath).mockResolvedValue('C:/p/new.msproj')
    vi.mocked(api.saveProject).mockResolvedValue(snapshot({ projectPath: 'C:/p/new.msproj' }))
    await store.saveAs()
    expect(api.saveProject).toHaveBeenCalledWith('C:/p/new.msproj')
  })
})

describe('splitting and selection', () => {
  it('splitSelectedAtPlayhead selects the new right half found by diffing ids', async () => {
    // Arrange
    await started()
    store.selectClip(10)
    store.position = 2 * SR
    const afterSplit = snapshot({
      project: {
        ...project(2 * SR),
        tracks: [
          {
            ...project().tracks[0]!,
            clips: [
              { id: 10, sourceId: 1, start: 0, offset: 0, length: 2 * SR, gainDb: 0, fadeIn: 0, fadeOut: 0 },
              { id: 11, sourceId: 1, start: 2 * SR, offset: 2 * SR, length: 2 * SR, gainDb: 0, fadeIn: 0, fadeOut: 0 },
            ],
          },
        ],
      },
    })
    vi.mocked(api.applyEdit).mockResolvedValue(afterSplit)
    // Act
    await store.splitSelectedAtPlayhead()
    // Assert
    expect(vi.mocked(api.applyEdit).mock.calls[0]?.[0]).toMatchObject({ type: 'splitClip', clip: 10, at: 2 * SR })
    expect(store.selection.clipId).toBe(11)
  })

  it('splitSelectedAtPlayhead refuses when the playhead is outside the clip', async () => {
    await started()
    store.selectClip(10)
    store.position = 10 * SR
    await store.splitSelectedAtPlayhead()
    expect(api.applyEdit).not.toHaveBeenCalled()
    expect(store.toasts.at(-1)).toMatchObject({ kind: 'warn' })
  })

  it('a selection pointing at a removed clip is cleared by the next snapshot', async () => {
    await started()
    store.selectClip(10)
    vi.mocked(api.applyEdit).mockResolvedValue(
      snapshot({ project: { ...project(), tracks: [{ ...project().tracks[0]!, clips: [] }] } }),
    )
    await store.deleteSelection()
    expect(store.selection.clipId).toBeNull()
  })
})

describe('toasts', () => {
  it('identical toasts within two seconds are deduplicated', () => {
    store.toast('warn', 'same')
    store.toast('warn', 'same')
    expect(store.toasts).toHaveLength(1)
  })

  it('keeps at most four toasts, dropping the oldest', () => {
    for (const text of ['a', 'b', 'c', 'd', 'e']) store.toast('info', text)
    expect(store.toasts.map((t) => t.text)).toEqual(['b', 'c', 'd', 'e'])
  })

  it('info toasts expire after four seconds while errors stay until dismissed', () => {
    store.toast('info', 'hello')
    store.toast('error', 'broken')
    vi.advanceTimersByTime(4000)
    expect(store.toasts.map((t) => t.kind)).toEqual(['error'])
  })

  it('dismissToast removes only the chosen toast', () => {
    store.toast('error', 'one')
    store.toast('error', 'two')
    const first = store.toasts[0]!
    store.dismissToast(first.id)
    expect(store.toasts.map((t) => t.text)).toEqual(['two'])
  })
})

describe('zoom', () => {
  it('zoomBy with a factor above one zooms in, so fpp gets smaller', async () => {
    await started()
    store.setViewport(1000, 600)
    const before = store.view.fpp
    store.zoomBy(2)
    expect(store.view.fpp).toBeLessThan(before)
  })
})
