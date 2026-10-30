import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

vi.mock('./store.svelte', () => ({
  store: {
    confirmRequest: null,
    export: { open: false },
    togglePlay: vi.fn(),
    returnToStart: vi.fn(),
    undo: vi.fn(),
    redo: vi.fn(),
    newProject: vi.fn(),
    openDialog: vi.fn(),
    save: vi.fn(),
    saveAs: vi.fn(),
    importDialog: vi.fn(),
    openExport: vi.fn(),
    closeExport: vi.fn(),
    zoomBy: vi.fn(),
    fitProject: vi.fn(),
    toggleMixer: vi.fn(),
    toggleFollow: vi.fn(),
    splitSelectedAtPlayhead: vi.fn(),
    deleteSelection: vi.fn(),
    toggleLoop: vi.fn(),
    removeSelectedTrack: vi.fn(),
    selectClip: vi.fn(),
    resolveConfirm: vi.fn(),
  },
}))

import { store } from './store.svelte'
import { type KeyInput, installKeymap, isEditableTarget, mapKey } from './keymap'

function input(overrides: Partial<KeyInput> & { key: string }): KeyInput {
  return { ctrlKey: false, shiftKey: false, altKey: false, targetIsEditable: false, ...overrides }
}

describe('mapKey: transport and navigation', () => {
  it.each([
    ['Space toggles play', { key: ' ' }, 'togglePlay'],
    ['Enter returns to start', { key: 'Enter' }, 'returnToStart'],
    ['Home returns to start', { key: 'Home' }, 'returnToStart'],
    ['L toggles loop', { key: 'l' }, 'toggleLoop'],
    ['F toggles follow', { key: 'f' }, 'toggleFollow'],
  ])('%s', (_name, keys, action) => {
    // Arrange / Act
    const decision = mapKey(input(keys))
    // Assert
    expect(decision).toEqual({ action, preventDefault: true })
  })

  it('maps S and Shift+S to split at playhead', () => {
    expect(mapKey(input({ key: 's' })).action).toBe('splitAtPlayhead')
    expect(mapKey(input({ key: 'S', shiftKey: true })).action).toBe('splitAtPlayhead')
  })

  it('maps Delete and Backspace to delete the selection', () => {
    expect(mapKey(input({ key: 'Delete' })).action).toBe('deleteSelection')
    expect(mapKey(input({ key: 'Backspace' })).action).toBe('deleteSelection')
  })

  it('maps Escape to escape even inside a text field', () => {
    expect(mapKey(input({ key: 'Escape', targetIsEditable: true })).action).toBe('escape')
  })

  it('leaves the 0 key alone without Ctrl, since faders use it for reset', () => {
    expect(mapKey(input({ key: '0' }))).toEqual({ action: null, preventDefault: false })
  })

  it('leaves F2 to the track header', () => {
    expect(mapKey(input({ key: 'F2' })).action).toBeNull()
  })
})

describe('mapKey: Ctrl combinations', () => {
  it.each([
    ['Ctrl+N', 'n', 'newProject'],
    ['Ctrl+O', 'o', 'openProject'],
    ['Ctrl+S', 's', 'save'],
    ['Ctrl+I', 'i', 'importFiles'],
    ['Ctrl+E', 'e', 'openExport'],
    ['Ctrl+=', '=', 'zoomIn'],
    ['Ctrl+-', '-', 'zoomOut'],
    ['Ctrl+0', '0', 'zoomFit'],
    ['Ctrl+M', 'm', 'toggleMixer'],
    ['Ctrl+Z', 'z', 'undo'],
    ['Ctrl+Y', 'y', 'redo'],
  ])('%s maps to %s', (_name, key, action) => {
    expect(mapKey(input({ key, ctrlKey: true })).action).toBe(action)
  })

  it('maps Ctrl+Shift+Z to redo and Ctrl+Shift+S to save as', () => {
    expect(mapKey(input({ key: 'Z', ctrlKey: true, shiftKey: true })).action).toBe('redo')
    expect(mapKey(input({ key: 'S', ctrlKey: true, shiftKey: true })).action).toBe('saveAs')
  })

  it('maps Ctrl+Shift+Delete to remove the selected track', () => {
    expect(mapKey(input({ key: 'Delete', ctrlKey: true, shiftKey: true })).action).toBe('removeTrack')
  })

  it('maps Ctrl+Shift+= to zoom in through the plus key', () => {
    expect(mapKey(input({ key: '+', ctrlKey: true, shiftKey: true })).action).toBe('zoomIn')
  })

  it('blocks unmapped Ctrl combinations so browser defaults do not run', () => {
    expect(mapKey(input({ key: 'p', ctrlKey: true }))).toEqual({ action: null, preventDefault: true })
  })

  it('ignores Alt combinations, which belong to the timeline canvas', () => {
    expect(mapKey(input({ key: 'ArrowLeft', altKey: true }))).toEqual({
      action: null,
      preventDefault: false,
    })
  })
})

describe('mapKey: editable targets', () => {
  it('ignores plain keys while a text field has focus', () => {
    expect(mapKey(input({ key: ' ', targetIsEditable: true }))).toEqual({
      action: null,
      preventDefault: false,
    })
  })

  it('still runs app Ctrl combinations from a text field', () => {
    expect(mapKey(input({ key: 's', ctrlKey: true, targetIsEditable: true })).action).toBe('save')
  })

  it('leaves undo, redo, select-all, copy, paste and cut to the field', () => {
    for (const key of ['z', 'y', 'a', 'c', 'v', 'x']) {
      expect(mapKey(input({ key, ctrlKey: true, targetIsEditable: true }))).toEqual({
        action: null,
        preventDefault: false,
      })
    }
  })

  it('blocks Ctrl+A on the instrument surface', () => {
    expect(mapKey(input({ key: 'a', ctrlKey: true })).preventDefault).toBe(true)
  })
})

describe('mapKey: browser defaults', () => {
  it('blocks F5 so the page does not reload', () => {
    expect(mapKey(input({ key: 'F5' }))).toEqual({ action: null, preventDefault: true })
  })

  it('ignores unmapped plain keys', () => {
    expect(mapKey(input({ key: '[' })).preventDefault).toBe(false)
    expect(mapKey(input({ key: 'ArrowLeft' })).action).toBeNull()
  })
})

describe('isEditableTarget', () => {
  it.each([
    ['INPUT', {}],
    ['TEXTAREA', {}],
    ['SELECT', {}],
  ])('treats a %s element as editable', (tagName) => {
    expect(isEditableTarget({ tagName, getAttribute: () => null } as unknown as EventTarget)).toBe(true)
  })

  it('treats contenteditable elements as editable', () => {
    const element = { tagName: 'DIV', isContentEditable: true, getAttribute: () => null }
    expect(isEditableTarget(element as unknown as EventTarget)).toBe(true)
  })

  it('treats slider roles as editable so arrows and Home stay with the control', () => {
    const element = { tagName: 'DIV', getAttribute: (name: string) => (name === 'role' ? 'slider' : null) }
    expect(isEditableTarget(element as unknown as EventTarget)).toBe(true)
  })

  it('does not treat buttons or plain elements as editable', () => {
    const button = { tagName: 'BUTTON', getAttribute: () => null }
    expect(isEditableTarget(button as unknown as EventTarget)).toBe(false)
    expect(isEditableTarget(null)).toBe(false)
  })
})

describe('installKeymap', () => {
  let target: EventTarget
  let win: Window
  let dispose: () => void

  beforeEach(() => {
    vi.clearAllMocks()
    store.confirmRequest = null
    store.export = { open: false } as typeof store.export
    target = new EventTarget()
    win = target as unknown as Window
    dispose = installKeymap(win)
  })

  afterEach(() => {
    dispose()
  })

  function press(key: string): Event {
    const event = new Event('keydown', { cancelable: true })
    Object.assign(event, { key })
    target.dispatchEvent(event)
    return event
  }

  it('dispatches the mapped action to the store and prevents the default', () => {
    const event = press(' ')
    expect(store.togglePlay).toHaveBeenCalledOnce()
    expect(event.defaultPrevented).toBe(true)
  })

  it('does not handle unmapped keys', () => {
    const event = press('q')
    expect(event.defaultPrevented).toBe(false)
    expect(store.togglePlay).not.toHaveBeenCalled()
  })

  it('stops handling keys after the disposer runs', () => {
    dispose()
    press(' ')
    expect(store.togglePlay).not.toHaveBeenCalled()
  })

  it('ignores everything except Escape while a confirm dialog is open', () => {
    store.confirmRequest = { request: { title: '', message: '', buttons: [] }, resolve: () => undefined }
    press(' ')
    expect(store.togglePlay).not.toHaveBeenCalled()
    press('Escape')
    expect(store.resolveConfirm).toHaveBeenCalledWith(null)
  })

  it('closes the export dialog on Escape when it is not running', () => {
    store.export = { open: true } as typeof store.export
    press('Escape')
    expect(store.closeExport).toHaveBeenCalledOnce()
  })

  it('deselects the clip on Escape when no dialog is open', () => {
    press('Escape')
    expect(store.selectClip).toHaveBeenCalledWith(null)
  })
})
