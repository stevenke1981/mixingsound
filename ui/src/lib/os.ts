// Native file dialogs and OS file drag-and-drop (ui-spec §3.4). Thin wrappers over Tauri plugins;
// classification is pure so the store can be tested without a window.

import { getCurrentWebview } from '@tauri-apps/api/webview'
import { open, save } from '@tauri-apps/plugin-dialog'

export const AUDIO_EXTENSIONS: readonly string[] = ['wav', 'mp3', 'flac']
export const PROJECT_EXTENSION = 'msproj'
export const EXPORT_EXTENSION = 'wav'

export interface ClassifiedPaths {
  audio: string[]
  projects: string[]
  skipped: string[]
}

export interface FileDropHandlers {
  enter(fileCount: number): void
  leave(): void
  drop(paths: string[]): void
}

function extensionOf(path: string): string {
  const name = path.split(/[\\/]/).pop() ?? ''
  const dot = name.lastIndexOf('.')
  return dot > 0 ? name.slice(dot + 1).toLowerCase() : ''
}

function withExtension(path: string, extension: string): string {
  return extensionOf(path) === extension ? path : `${path}.${extension}`
}

/** Splits dropped or chosen paths by extension (case-insensitive). */
export function classifyPaths(paths: readonly string[]): ClassifiedPaths {
  const byExtension = (extensions: readonly string[]): string[] =>
    paths.filter((path) => extensions.includes(extensionOf(path)))
  const audio = byExtension(AUDIO_EXTENSIONS)
  const projects = byExtension([PROJECT_EXTENSION])
  const skipped = paths.filter((path) => !audio.includes(path) && !projects.includes(path))
  return { audio, projects, skipped }
}

/** Multi-select audio picker. Resolves to `[]` when the user cancels. */
export async function pickAudioFiles(): Promise<string[]> {
  const selected = await open({
    multiple: true,
    directory: false,
    filters: [{ name: 'Audio', extensions: [...AUDIO_EXTENSIONS] }],
  })
  return selected ?? []
}

/** Single project picker. Resolves to `null` when the user cancels. */
export async function pickProjectFile(): Promise<string | null> {
  return open({
    multiple: false,
    directory: false,
    filters: [{ name: 'MixingSound project', extensions: [PROJECT_EXTENSION] }],
  })
}

/** Save-as picker for projects. Adds the `.msproj` extension when missing. */
export async function pickProjectSavePath(defaultName: string): Promise<string | null> {
  const path = await save({
    defaultPath: defaultName,
    filters: [{ name: 'MixingSound project', extensions: [PROJECT_EXTENSION] }],
  })
  return path === null ? null : withExtension(path, PROJECT_EXTENSION)
}

/** Save picker for WAV export. Adds the `.wav` extension when missing. */
export async function pickExportPath(defaultName: string): Promise<string | null> {
  const path = await save({
    defaultPath: defaultName,
    filters: [{ name: 'WAV audio', extensions: [EXPORT_EXTENSION] }],
  })
  return path === null ? null : withExtension(path, EXPORT_EXTENSION)
}

/** Subscribes to OS file drags over the window. Resolves to the unlisten function. */
export function subscribeFileDrop(handlers: FileDropHandlers): Promise<() => void> {
  return getCurrentWebview().onDragDropEvent((event) => {
    const payload = event.payload
    if (payload.type === 'enter') handlers.enter(payload.paths.length)
    else if (payload.type === 'leave') handlers.leave()
    else if (payload.type === 'drop') handlers.drop(payload.paths)
  })
}
