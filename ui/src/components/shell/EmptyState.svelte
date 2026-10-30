<script lang="ts">
  import Icon from './Icon.svelte'
  import { store } from '../../lib/store.svelte'
</script>

<section class="empty" aria-label="Empty project">
  <div class="dropzone empty-zone">
    <Icon name="import" size={48} />
    <h2 class="empty-title">Drop audio files here</h2>
    <p class="empty-hint">WAV · MP3 · FLAC — one track per file</p>
    <div class="empty-action">
      <button type="button" class="btn btn-primary" onclick={() => store.importDialog()}>
        Import audio…
        <span class="kbd">Ctrl+I</span>
      </button>
    </div>
  </div>
</section>

<style>
  .empty {
    display: grid;
    place-items: center;
    min-height: 0;
    padding: var(--sp-6);
  }

  .empty-zone {
    width: min(520px, 100%);
    height: min(220px, 100%);
    padding: var(--sp-6);
    gap: var(--sp-3);
    background: var(--bg-0);
  }

  .empty-title {
    color: var(--text);
    font: var(--fw-semibold) var(--fs-xl) / var(--lh-tight) var(--font-ui);
  }

  .empty-hint {
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }

  .empty-action {
    display: flex;
    justify-content: center;
    margin-top: var(--sp-3);
  }
</style>
