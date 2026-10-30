<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import type { Track } from '../../lib/types'
  import Icon from '../shell/Icon.svelte'
  import TrackMenu from './TrackMenu.svelte'
  import { formatTrackSummary } from './drawClip'

  interface Props {
    track: Track
    /** Zero-based position in the track list. */
    index: number
    count: number
    selected: boolean
  }
  let { track, index, count, selected }: Props = $props()

  let renaming = $state(false)
  let draft = $state('')
  let menuOpen = $state(false)
  let menuButton: HTMLButtonElement | undefined = $state()

  const summary = $derived(formatTrackSummary(track.gainDb, track.pan))

  function select(): void {
    store.selectTrack(track.id)
  }

  function startRename(): void {
    menuOpen = false
    draft = track.name
    renaming = true
  }

  /** Commits once: Enter, blur and Escape all end the rename, so only the first call acts. */
  function commitRename(): void {
    if (!renaming) return
    renaming = false
    const name = draft.trim()
    if (name !== '' && name !== track.name) void store.renameTrack(track.id, name)
  }

  function onRenameKey(e: KeyboardEvent): void {
    if (e.key === 'Enter') {
      e.preventDefault()
      commitRename()
    } else if (e.key === 'Escape') {
      e.preventDefault()
      e.stopPropagation()
      renaming = false
    }
  }

  /** F2 on the focused name renames; Enter and Space already select via the button's click. */
  function onNameKey(e: KeyboardEvent): void {
    if (e.key !== 'F2') return
    e.preventDefault()
    startRename()
  }

  function focusAndSelect(node: HTMLInputElement): void {
    node.focus()
    node.select()
  }

  function toggleMenu(e: MouseEvent): void {
    e.stopPropagation()
    menuOpen = !menuOpen
  }

  function closeMenu(restoreFocus: boolean): void {
    menuOpen = false
    if (restoreFocus) menuButton?.focus()
  }

  function toggleMute(e: MouseEvent): void {
    e.stopPropagation()
    void store.setTrackMute(track.id, !track.mute)
  }

  function toggleSolo(e: MouseEvent): void {
    e.stopPropagation()
    void store.setTrackSolo(track.id, !track.solo)
  }
</script>

<div
  class="track-head"
  class:is-selected={selected}
  style:--track-color={track.color}
  role="group"
  aria-label="Track {track.name}"
>
  <span class="bar" aria-hidden="true"></span>
  <div class="body">
    <div class="row-name">
      {#if renaming}
        <input
          class="field name-input"
          aria-label="Track name"
          bind:value={draft}
          use:focusAndSelect
          onkeydown={onRenameKey}
          onblur={commitRename}
        />
      {:else}
        <button
          type="button"
          class="name truncate"
          title="Double-click or F2 to rename"
          onclick={select}
          ondblclick={startRename}
          onkeydown={onNameKey}
        >{track.name}</button>
      {/if}
      <button
        type="button"
        class="btn btn-icon btn-ghost menu-btn"
        bind:this={menuButton}
        aria-label="Track options for {track.name}"
        aria-haspopup="menu"
        aria-expanded={menuOpen}
        onclick={toggleMenu}
      >
        <Icon name="more" size={16} />
      </button>
    </div>
    <div class="row-ms">
      <button
        type="button"
        class="ms"
        class:is-mute={track.mute}
        aria-pressed={track.mute}
        aria-label="Mute {track.name}"
        onclick={toggleMute}
      >M</button>
      <button
        type="button"
        class="ms"
        class:is-solo={track.solo}
        aria-pressed={track.solo}
        aria-label="Solo {track.name}"
        onclick={toggleSolo}
      >S</button>
      <span class="summary num">{summary}</span>
    </div>
  </div>
  {#if menuOpen && menuButton}
    <TrackMenu
      {track}
      {index}
      {count}
      anchor={menuButton}
      onClose={closeMenu}
      onRename={startRename}
    />
  {/if}
</div>

<style>
  .track-head {
    position: relative;
    display: grid;
    grid-template-columns: var(--bar-w, 4px) minmax(0, 1fr);
    height: var(--lane-h);
    background: var(--bg-2);
    border-bottom: 1px solid var(--line);
    box-shadow: inset 0 1px 0 var(--line-hi);
  }

  .track-head:hover {
    background: var(--bg-3);
  }

  .track-head.is-selected {
    --bar-w: 6px;
    background: var(--bg-3);
  }

  .bar {
    background: var(--track-color);
  }

  .body {
    display: grid;
    grid-template-rows: 24px 24px minmax(0, 1fr);
    min-width: 0;
    padding: 0 var(--sp-4) 0 var(--sp-5);
  }

  .row-name,
  .row-ms {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-width: 0;
  }

  .name,
  .name-input {
    flex: 1;
    min-width: 0;
    height: 24px;
    text-align: left;
    color: var(--text);
    font: var(--fw-semibold) var(--fs-md) / 1 var(--font-ui);
  }

  .name {
    padding: 0;
    background: transparent;
    border: 0;
  }

  .name-input {
    padding: 0 var(--sp-3);
  }

  .menu-btn {
    width: 24px;
    height: 24px;
    min-width: 24px;
  }

  .summary {
    min-width: 0;
    overflow: hidden;
    color: var(--text-muted);
    font-size: var(--fs-xs);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
