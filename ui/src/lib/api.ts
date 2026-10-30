// Typed wrappers over Tauri IPC. The only module that calls `invoke`/`listen` directly.
// Command names/arguments mirror src-tauri/src/commands.rs.

import { invoke } from '@tauri-apps/api/core'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type {
  AppSnapshot,
  BounceOptions,
  BounceReport,
  Edit,
  EngineStatus,
  SourceId,
  WaveformColumns,
} from './types'

export const api = {
  init: () => invoke<AppSnapshot>('init'),
  newProject: () => invoke<AppSnapshot>('new_project'),
  importFiles: (paths: string[]) => invoke<AppSnapshot>('import_files', { paths }),
  applyEdit: (edit: Edit) => invoke<AppSnapshot>('apply_edit', { edit }),
  /** Call on pointer-up after a continuous gesture so the next edit is a new undo step. */
  endGesture: () => invoke<void>('end_gesture'),
  undo: () => invoke<AppSnapshot>('undo'),
  redo: () => invoke<AppSnapshot>('redo'),
  play: () => invoke<void>('transport_play'),
  pause: () => invoke<void>('transport_pause'),
  stop: () => invoke<void>('transport_stop'),
  seek: (frame: number) => invoke<void>('transport_seek', { frame: Math.max(0, Math.round(frame)) }),
  setLoop: (start: number | null, end: number | null) =>
    invoke<void>('transport_set_loop', { start, end }),
  getWaveform: (sourceId: SourceId, start: number, end: number, columns: number) =>
    invoke<WaveformColumns>('get_waveform', { sourceId, start, end, columns }),
  /** `path: null` saves to the current project path. */
  saveProject: (path: string | null) => invoke<AppSnapshot>('save_project', { path }),
  openProject: (path: string) => invoke<AppSnapshot>('open_project', { path }),
  exportWav: (path: string, options: BounceOptions) =>
    invoke<BounceReport>('export_wav', { path, options }),
}

export function onEngineStatus(cb: (s: EngineStatus) => void): Promise<UnlistenFn> {
  return listen<EngineStatus>('engine-status', (e) => cb(e.payload))
}

export function onExportProgress(cb: (fraction: number) => void): Promise<UnlistenFn> {
  return listen<{ fraction: number }>('export-progress', (e) => cb(e.payload.fraction))
}

/** IPC errors arrive as plain strings. */
export function errorMessage(e: unknown): string {
  if (typeof e === 'string') return e
  if (e instanceof Error) return e.message
  return 'Unexpected error'
}
