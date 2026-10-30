<script lang="ts">
  import Icon from './Icon.svelte'
  import TimeDisplay from './TimeDisplay.svelte'
  import { store } from '../../lib/store.svelte'
  import { ZOOM_STEP, fppToSlider, sliderToFpp } from '../../lib/geometry'
  import { fileStem } from '../../lib/format'

  const projectPath = $derived(store.snapshot?.projectPath ?? null)
  const projectName = $derived(projectPath === null ? 'Untitled' : fileStem(projectPath) || 'Untitled')
  const hasClips = $derived(store.project?.tracks.some((t) => t.clips.length > 0) ?? false)
  const noDevice = $derived(!store.hasDevice)
  const saveDisabled = $derived(store.snapshot === null || (!store.dirty && projectPath !== null))
  const zoomPosition = $derived(fppToSlider(store.view.fpp, store.sampleRate))

  function onZoomSlider(event: Event): void {
    const value = Number((event.currentTarget as HTMLInputElement).value)
    store.setFpp(sliderToFpp(value, store.sampleRate))
  }
</script>

<header class="transport" role="toolbar" aria-label="Transport">
  <div class="group">
    <button type="button" class="btn btn-icon" aria-label="New" title="New (Ctrl+N)" onclick={() => store.newProject()}>
      <Icon name="new" />
    </button>
    <button type="button" class="btn btn-icon" aria-label="Open" title="Open (Ctrl+O)" onclick={() => store.openDialog()}>
      <Icon name="open" />
    </button>
    <button
      type="button"
      class="btn btn-icon"
      aria-label="Save"
      title="Save (Ctrl+S)"
      disabled={saveDisabled}
      onclick={() => store.save()}
    >
      <Icon name="save" />
    </button>
    <button type="button" class="btn btn-icon" aria-label="Import" title="Import (Ctrl+I)" onclick={() => store.importDialog()}>
      <Icon name="import" />
    </button>
    <button
      type="button"
      class="btn btn-icon"
      class:btn-primary={hasClips}
      aria-label="Export"
      title="Export (Ctrl+E)"
      disabled={!hasClips}
      onclick={() => store.openExport()}
    >
      <Icon name="export" />
    </button>
  </div>

  <span class="divider-v" aria-hidden="true"></span>

  <div class="group">
    <button
      type="button"
      class="btn btn-icon"
      aria-label="Undo"
      title="Undo (Ctrl+Z)"
      disabled={!store.canUndo}
      onclick={() => store.undo()}
    >
      <Icon name="undo" />
    </button>
    <button
      type="button"
      class="btn btn-icon"
      aria-label="Redo"
      title="Redo (Ctrl+Shift+Z / Ctrl+Y)"
      disabled={!store.canRedo}
      onclick={() => store.redo()}
    >
      <Icon name="redo" />
    </button>
  </div>

  <span class="divider-v" aria-hidden="true"></span>

  <div class="group">
    <button
      type="button"
      class="btn btn-icon"
      aria-label="Return to start"
      title="Return to start (Home / Enter)"
      disabled={noDevice}
      onclick={() => store.returnToStart()}
    >
      <Icon name="return" />
    </button>
    <button
      type="button"
      class="btn btn-play play-btn"
      class:is-playing={store.playing}
      aria-label={store.playing ? 'Pause' : 'Play'}
      title={store.playing ? 'Pause (Space)' : 'Play (Space)'}
      disabled={noDevice}
      onclick={() => store.togglePlay()}
    >
      <Icon name={store.playing ? 'pause' : 'play'} />
    </button>
    <button
      type="button"
      class="btn btn-icon"
      aria-label="Stop"
      title="Stop"
      disabled={noDevice}
      onclick={() => store.stop()}
    >
      <Icon name="stop" />
    </button>
    <button
      type="button"
      class="btn btn-icon"
      class:is-on={store.loopEnabled}
      aria-label="Loop"
      aria-pressed={store.loopEnabled}
      title="Loop (L)"
      onclick={() => store.toggleLoop()}
    >
      <Icon name="loop" />
    </button>
  </div>

  <span class="divider-v" aria-hidden="true"></span>

  <div class="group">
    <TimeDisplay frames={store.position} sampleRate={store.sampleRate} />
  </div>

  <span class="divider-v" aria-hidden="true"></span>

  <div class="group">
    <button type="button" class="btn btn-icon" aria-label="Zoom out" title="Zoom out (Ctrl+−)" onclick={() => store.zoomBy(1 / ZOOM_STEP)}>
      <Icon name="zoom-out" />
    </button>
    <input
      type="range"
      class="zoom-slider"
      min="0"
      max="1"
      step="0.001"
      value={zoomPosition}
      aria-label="Zoom level"
      oninput={onZoomSlider}
    />
    <button type="button" class="btn btn-icon" aria-label="Zoom in" title="Zoom in (Ctrl+=)" onclick={() => store.zoomBy(ZOOM_STEP)}>
      <Icon name="zoom-in" />
    </button>
    <button type="button" class="btn btn-icon" aria-label="Fit" title="Fit project (Ctrl+0)" onclick={() => store.fitProject()}>
      <Icon name="fit" />
    </button>
  </div>

  <div class="spacer"></div>

  <div class="project">
    <span class="project-name truncate" title={projectPath ?? 'Not saved yet'}>{projectName}</span>
    {#if store.dirty}
      <span class="dirty-dot" role="img" aria-label="Unsaved changes"></span>
    {/if}
  </div>

  {#if store.busy !== null}
    <div class="busy-bar" role="progressbar" aria-label="Working"></div>
  {/if}
</header>

<style>
  .transport {
    position: relative;
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    height: var(--transport-h);
    min-width: 0;
    padding: 0 var(--sp-5);
    overflow: hidden;
    background: var(--bg-1);
    border-bottom: 1px solid var(--line);
    box-shadow: inset 0 1px 0 var(--line-hi);
  }

  .group {
    display: flex;
    align-items: center;
    gap: var(--sp-2);
    flex: none;
  }

  .divider-v {
    flex: none;
    width: 1px;
    height: 24px;
    margin: 0 var(--sp-3);
    background: var(--line);
  }

  .play-btn {
    width: 44px;
    height: 32px;
    padding: 0;
  }

  .zoom-slider {
    width: 72px;
    margin: 0 var(--sp-2);
    accent-color: var(--accent);
  }

  .spacer {
    flex: 1;
  }

  .project {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    min-width: 0;
    max-width: 220px;
    font-size: var(--fs-md);
  }

  .project-name {
    min-width: 0;
  }

  .busy-bar {
    position: absolute;
    left: 0;
    right: 0;
    bottom: 0;
    height: 2px;
    overflow: hidden;
    background: var(--bg-3);
  }

  .busy-bar::after {
    content: '';
    display: block;
    width: 30%;
    height: 100%;
    background: var(--accent);
    animation: busy-sweep 1.2s var(--ease-inout) infinite;
  }

  @keyframes busy-sweep {
    from {
      transform: translateX(-100%);
    }
    to {
      transform: translateX(340%);
    }
  }
</style>
