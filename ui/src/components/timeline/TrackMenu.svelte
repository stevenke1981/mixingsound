<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import type { Track } from '../../lib/types'

  interface Props {
    track: Track
    index: number
    count: number
    /** The ⋯ button that opened the menu; the menu is placed under it. */
    anchor: HTMLElement
    /** `restoreFocus` is true when the menu closes by key, so focus returns to the anchor. */
    onClose: (restoreFocus: boolean) => void
    onRename: () => void
  }
  let { track, index, count, anchor, onClose, onRename }: Props = $props()

  const SWATCH_COUNT = 8
  const HEX = /^#[0-9a-f]{6}$/
  const EDGE_PX = 8
  const DELETE_TOAST = 'Track removed – Ctrl+Z to undo'

  let menuEl: HTMLDivElement | undefined = $state()

  /** Reads `--track-1..8` once; the palette tokens are the engine's track colours (ui-spec §2.3). */
  function readTrackSwatches(): string[] {
    const style = getComputedStyle(document.documentElement)
    const out: string[] = []
    for (let i = 1; i <= SWATCH_COUNT; i++) {
      const hex = style.getPropertyValue(`--track-${i}`).trim().toLowerCase()
      if (HEX.test(hex)) out.push(hex)
    }
    return out
  }
  const swatches = readTrackSwatches()
  const currentColour = $derived(track.color.toLowerCase())

  function pickColour(hex: string): void {
    void store.setTrackColor(track.id, hex)
    onClose(true)
  }

  function rename(): void {
    onRename()
  }

  function moveUp(): void {
    void store.moveTrack(track.id, index - 1)
    onClose(false)
  }

  function moveDown(): void {
    void store.moveTrack(track.id, index + 1)
    onClose(false)
  }

  async function remove(): Promise<void> {
    const id = track.id
    onClose(false)
    await store.removeTrack(id)
    if (!store.trackById(id)) store.toast('info', DELETE_TOAST)
  }

  function menuItems(): HTMLButtonElement[] {
    return Array.from(menuEl?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])
  }

  function onMenuKey(e: KeyboardEvent): void {
    if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      onClose(true)
      return
    }
    if (e.key !== 'ArrowDown' && e.key !== 'ArrowUp') return
    e.preventDefault()
    const items = menuItems()
    if (items.length === 0) return
    const current = items.indexOf(document.activeElement as HTMLButtonElement)
    const step = e.key === 'ArrowDown' ? 1 : -1
    const next = (current + step + items.length) % items.length
    items[next]?.focus()
  }

  // Place the menu under the anchor, flipping above when there is no room below.
  $effect(() => {
    const node = menuEl
    if (!node) return
    const rect = anchor.getBoundingClientRect()
    const width = node.offsetWidth
    const height = node.offsetHeight
    const left = Math.min(Math.max(EDGE_PX, rect.right - width), window.innerWidth - width - EDGE_PX)
    const below = rect.bottom + 4
    const fitsBelow = below + height <= window.innerHeight - EDGE_PX
    const top = fitsBelow ? below : Math.max(EDGE_PX, rect.top - height - 4)
    node.style.left = `${left}px`
    node.style.top = `${top}px`
    menuItems()[0]?.focus()
  })

  // Close on any pointer press outside the menu and its anchor.
  $effect(() => {
    const onDown = (e: PointerEvent): void => {
      const target = e.target
      if (target instanceof Node && (menuEl?.contains(target) || anchor.contains(target))) return
      onClose(false)
    }
    window.addEventListener('pointerdown', onDown, true)
    return () => window.removeEventListener('pointerdown', onDown, true)
  })
</script>

<div
  class="menu track-menu"
  bind:this={menuEl}
  role="menu"
  aria-label="Track {track.name}"
  tabindex="-1"
  onkeydown={onMenuKey}
>
  <button type="button" class="menu-item" role="menuitem" onclick={rename}>Rename</button>
  <div class="menu-section" role="group" aria-label="Colour">
    <span class="section-label label-caps">Colour</span>
    <div class="swatches">
      {#each swatches as hex, i (hex)}
        <button
          type="button"
          class="swatch"
          class:is-current={hex === currentColour}
          style:background={hex}
          aria-label="Colour {i + 1}"
          aria-pressed={hex === currentColour}
          title="Colour {i + 1}"
          onclick={() => pickColour(hex)}
        ></button>
      {/each}
    </div>
  </div>
  <div class="menu-sep" role="separator"></div>
  <button type="button" class="menu-item" role="menuitem" disabled={index <= 0} onclick={moveUp}>
    Move up
  </button>
  <button type="button" class="menu-item" role="menuitem" disabled={index >= count - 1} onclick={moveDown}>
    Move down
  </button>
  <div class="menu-sep" role="separator"></div>
  <button type="button" class="menu-item btn-danger" role="menuitem" onclick={remove}>Delete track</button>
</div>

<style>
  .track-menu {
    position: fixed;
    z-index: var(--z-popover);
    display: grid;
    gap: 2px;
    min-width: 176px;
  }

  .menu-section {
    display: grid;
    gap: var(--sp-3);
    padding: var(--sp-3) var(--sp-4);
  }

  .section-label {
    font-size: var(--fs-2xs);
  }

  .swatches {
    display: flex;
    flex-wrap: wrap;
    gap: var(--sp-3);
  }

  .swatch {
    width: 16px;
    height: 16px;
    padding: 0;
    border: 1px solid var(--line-strong);
    border-radius: var(--r-pill);
    cursor: default;
  }

  .swatch:hover {
    border-color: var(--text);
  }

  .swatch.is-current {
    box-shadow:
      0 0 0 2px var(--bg-2),
      0 0 0 4px var(--select);
  }

  .menu-sep {
    height: 1px;
    margin: var(--sp-2) var(--sp-3);
    background: var(--line);
  }
</style>
