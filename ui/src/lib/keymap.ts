// Global keyboard map (ui-spec §3.3). `mapKey` is pure; `installKeymap` wires it to the window.
// Canvas-local keys (nudge, [ ], Alt+arrows) belong to the timeline and are ignored here.

import { ZOOM_STEP } from './geometry'
import { store } from './store.svelte'

export type KeyAction =
  | 'togglePlay'
  | 'returnToStart'
  | 'escape'
  | 'undo'
  | 'redo'
  | 'newProject'
  | 'openProject'
  | 'save'
  | 'saveAs'
  | 'importFiles'
  | 'openExport'
  | 'zoomIn'
  | 'zoomOut'
  | 'zoomFit'
  | 'toggleMixer'
  | 'toggleFollow'
  | 'splitAtPlayhead'
  | 'deleteSelection'
  | 'toggleLoop'
  | 'removeTrack'

export interface KeyInput {
  key: string
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
  /** True when focus is in a text field, slider or other key-consuming control. */
  targetIsEditable: boolean
}

/** `preventDefault` is set for every key the app owns, including browser defaults it blocks. */
export interface KeyDecision {
  action: KeyAction | null
  preventDefault: boolean
}

export type KeyTarget = Pick<Window, 'addEventListener' | 'removeEventListener'>

const IGNORE: KeyDecision = { action: null, preventDefault: false }
const BLOCK: KeyDecision = { action: null, preventDefault: true }

function act(action: KeyAction): KeyDecision {
  return { action, preventDefault: true }
}

const CTRL_ACTIONS: Partial<Record<string, KeyAction>> = {
  n: 'newProject',
  o: 'openProject',
  s: 'save',
  i: 'importFiles',
  e: 'openExport',
  '=': 'zoomIn',
  '+': 'zoomIn',
  '-': 'zoomOut',
  _: 'zoomOut',
  '0': 'zoomFit',
  m: 'toggleMixer',
  z: 'undo',
  y: 'redo',
}

const CTRL_SHIFT_ACTIONS: Partial<Record<string, KeyAction>> = {
  s: 'saveAs',
  z: 'redo',
  Delete: 'removeTrack',
}

/** Undo/redo inside a text field stay with the field's native history. */
const NATIVE_TEXT_CTRL_KEYS: ReadonlySet<string> = new Set(['z', 'y'])

const PLAIN_ACTIONS: Partial<Record<string, KeyAction>> = {
  ' ': 'togglePlay',
  Enter: 'returnToStart',
  Home: 'returnToStart',
  Delete: 'deleteSelection',
  Backspace: 'deleteSelection',
  s: 'splitAtPlayhead',
  l: 'toggleLoop',
  f: 'toggleFollow',
}

const EDITABLE_TAGS: ReadonlySet<string> = new Set(['INPUT', 'TEXTAREA', 'SELECT'])
const KEY_CONSUMING_ROLES: ReadonlySet<string> = new Set([
  'textbox',
  'slider',
  'spinbutton',
  'combobox',
])

function normalizeKey(key: string): string {
  return key.length === 1 ? key.toLowerCase() : key
}

function mapCtrlKey(key: string, input: KeyInput): KeyDecision {
  const action = input.shiftKey
    ? (CTRL_SHIFT_ACTIONS[key] ?? CTRL_ACTIONS[key])
    : CTRL_ACTIONS[key]
  if (input.targetIsEditable) {
    return action && !NATIVE_TEXT_CTRL_KEYS.has(key) ? act(action) : IGNORE
  }
  return action ? act(action) : BLOCK
}

/** Pure mapping from a key press to an app action. */
export function mapKey(input: KeyInput): KeyDecision {
  const key = normalizeKey(input.key)
  if (key === 'Escape') return act('escape')
  if (input.altKey) return IGNORE
  if (input.ctrlKey) return mapCtrlKey(key, input)
  if (input.targetIsEditable) return IGNORE
  if (key === 'F5') return BLOCK
  const action = PLAIN_ACTIONS[key]
  return action ? act(action) : IGNORE
}

/** True for text fields and key-consuming widgets (sliders keep arrows, Home, End). */
export function isEditableTarget(target: EventTarget | null): boolean {
  if (!isElementLike(target)) return false
  if (EDITABLE_TAGS.has(target.tagName) || target.isContentEditable === true) return true
  return KEY_CONSUMING_ROLES.has(target.getAttribute('role') ?? '')
}

interface ElementLike {
  tagName: string
  isContentEditable?: boolean
  getAttribute(name: string): string | null
}

function isElementLike(target: EventTarget | null): target is EventTarget & ElementLike {
  return target !== null && typeof (target as Partial<ElementLike>).tagName === 'string'
}

/** Enter and Space on a focused button already click it; the global map must not double-fire. */
function isNativeButtonActivation(event: KeyboardEvent): boolean {
  const target = event.target
  if (!isElementLike(target) || target.tagName !== 'BUTTON') return false
  return event.key === 'Enter' || event.key === ' '
}

function handleEscape(): void {
  if (store.confirmRequest) {
    store.resolveConfirm(null)
    return
  }
  if (store.export.open) {
    store.closeExport()
    return
  }
  store.selectClip(null)
}

const HANDLERS: Record<KeyAction, () => void> = {
  togglePlay: () => void store.togglePlay(),
  returnToStart: () => void store.returnToStart(),
  escape: handleEscape,
  undo: () => void store.undo(),
  redo: () => void store.redo(),
  newProject: () => void store.newProject(),
  openProject: () => void store.openDialog(),
  save: () => void store.save(),
  saveAs: () => void store.saveAs(),
  importFiles: () => void store.importDialog(),
  openExport: () => store.openExport(),
  zoomIn: () => store.zoomBy(ZOOM_STEP),
  zoomOut: () => store.zoomBy(1 / ZOOM_STEP),
  zoomFit: () => store.fitProject(),
  toggleMixer: () => store.toggleMixer(),
  toggleFollow: () => store.toggleFollow(),
  splitAtPlayhead: () => void store.splitSelectedAtPlayhead(),
  deleteSelection: () => void store.deleteSelection(),
  toggleLoop: () => void store.toggleLoop(),
  removeTrack: () => void store.removeSelectedTrack(),
}

function isModalOpen(): boolean {
  return store.confirmRequest !== null || store.export.open
}

/** Installs the global keydown handler. Returns the disposer. */
export function installKeymap(target: KeyTarget = window): () => void {
  const onKeyDown = (event: KeyboardEvent): void => {
    // Modal dialogs own the keyboard; only Escape (close) passes through.
    if (isModalOpen() && event.key !== 'Escape') return
    if (isNativeButtonActivation(event)) return
    const decision = mapKey({
      key: event.key,
      ctrlKey: event.ctrlKey,
      shiftKey: event.shiftKey,
      altKey: event.altKey,
      targetIsEditable: isEditableTarget(event.target),
    })
    if (decision.preventDefault) event.preventDefault()
    if (decision.action) HANDLERS[decision.action]()
  }
  target.addEventListener('keydown', onKeyDown)
  return () => target.removeEventListener('keydown', onKeyDown)
}
