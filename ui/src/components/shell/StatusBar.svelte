<script lang="ts">
  import ClipInspector from './ClipInspector.svelte'
  import Icon from './Icon.svelte'
  import { store } from '../../lib/store.svelte'
  import { formatKhz } from '../../lib/format'

  const missing = $derived(store.snapshot?.missingSources ?? [])
  const clipSelected = $derived(
    store.selection.clipId !== null && store.clipById(store.selection.clipId) !== undefined,
  )
  const deviceName = $derived(store.snapshot?.deviceName ?? null)

  function reportMissing(): void {
    for (const id of missing) {
      store.toast('warn', `Missing file: ${store.sourceById(id)?.name ?? `source ${id}`}`)
    }
  }
</script>

<footer class="statusbar" role="status" aria-live="off">
  <div class="left">
    {#if clipSelected}
      <ClipInspector />
    {:else}
      <span class="hint">Space play · S split · Ctrl+wheel zoom</span>
    {/if}
  </div>

  <div class="right">
    {#if missing.length > 0}
      <button type="button" class="chip chip-warn" title="List missing files" onclick={reportMissing}>
        <Icon name="warn" size={12} />
        {missing.length} missing {missing.length === 1 ? 'file' : 'files'}
      </button>
    {/if}
    {#if store.snapshot}
      {#if deviceName !== null}
        <span class="chip"><span class="chip-dot ok"></span>{deviceName} · {formatKhz(store.sampleRate)} kHz</span>
      {:else}
        <span class="chip"><span class="chip-dot err"></span>No audio device</span>
      {/if}
    {/if}
    {#if store.status.error !== null}
      <span class="chip chip-error" title={store.status.error}>{store.status.error}</span>
    {/if}
  </div>
</footer>

<style>
  .statusbar {
    display: flex;
    align-items: center;
    gap: var(--sp-5);
    height: var(--statusbar-h);
    min-width: 0;
    padding: 0 var(--sp-5);
    overflow: hidden;
    color: var(--text-2);
    font-size: var(--fs-sm);
    background: var(--bg-1);
    border-top: 1px solid var(--line);
  }

  .left {
    display: flex;
    align-items: center;
    flex: 1;
    min-width: 0;
    overflow: hidden;
  }

  .right {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    flex: none;
  }

  .hint {
    color: var(--text-muted);
    white-space: nowrap;
  }

  .chip-warn {
    color: var(--meter-warn);
    border-color: var(--meter-warn);
    cursor: pointer;
  }

  .chip-error {
    max-width: 360px;
    color: var(--danger);
    border-color: var(--danger);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
</style>
