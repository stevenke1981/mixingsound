<script lang="ts">
  import { onMount } from 'svelte'
  import Transport from './components/shell/Transport.svelte'
  import EmptyState from './components/shell/EmptyState.svelte'
  import StatusBar from './components/shell/StatusBar.svelte'
  import ToastHost from './components/shell/ToastHost.svelte'
  import ExportDialog from './components/shell/ExportDialog.svelte'
  import ConfirmDialog from './components/shell/ConfirmDialog.svelte'
  import DropOverlay from './components/shell/DropOverlay.svelte'
  import Timeline from './components/timeline/Timeline.svelte'
  import MixerPanel from './components/mixer/MixerPanel.svelte'
  import { installKeymap } from './lib/keymap'
  import { store } from './lib/store.svelte'

  const hasTracks = $derived((store.project?.tracks.length ?? 0) > 0)

  onMount(() => {
    void store.init()
    const removeKeymap = installKeymap(window)
    return () => {
      removeKeymap()
      store.dispose()
    }
  })
</script>

<div class="shell" aria-busy={store.busy !== null}>
  <Transport />

  <div class="main">
    {#if hasTracks}
      <Timeline />
    {:else}
      <EmptyState />
    {/if}
  </div>

  <MixerPanel />
  <StatusBar />
</div>

<ToastHost />
<ExportDialog />
<ConfirmDialog />
<DropOverlay />

<div class="sr-only" aria-live="polite">{store.announcement}</div>

<style>
  .shell {
    display: grid;
    height: 100vh;
    min-width: 0;
    grid-template-rows: var(--transport-h) minmax(0, 1fr) auto var(--statusbar-h);
    overflow: hidden;
  }

  .main {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
</style>
