<script lang="ts">
  import Icon from './Icon.svelte'
  import { store } from '../../lib/store.svelte'

  const countText = $derived(`${store.dropCount} file${store.dropCount === 1 ? '' : 's'}`)
</script>

{#if store.dropActive}
  <div class="drop-overlay">
    <div class="dropzone is-over drop-zone">
      <Icon name="import" size={48} />
      <p class="drop-text">Drop to import {countText}</p>
    </div>
  </div>
{/if}

<style>
  .drop-overlay {
    position: fixed;
    inset: 0;
    z-index: var(--z-modal);
    display: grid;
    place-items: center;
    padding: var(--sp-6);
    background: var(--scrim);
    pointer-events: none;
  }

  .drop-zone {
    width: min(520px, 100%);
    height: min(260px, 100%);
    padding: var(--sp-6);
    background: var(--bg-1);
  }

  .drop-text {
    color: var(--text);
    font: var(--fw-semibold) var(--fs-xl) / var(--lh-tight) var(--font-ui);
  }
</style>
