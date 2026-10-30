<script lang="ts">
  import Icon from './Icon.svelte'
  import { store } from '../../lib/store.svelte'
</script>

<div class="toast-stack" aria-live="polite">
  {#each store.toasts as toast (toast.id)}
    <div class="toast {toast.kind}" role={toast.kind === 'error' ? 'alert' : 'status'}>
      <p class="toast-text">{toast.text}</p>
      <button
        type="button"
        class="toast-close"
        aria-label="Dismiss notification"
        title="Dismiss"
        onclick={() => store.dismissToast(toast.id)}
      >
        <Icon name="close" size={14} />
      </button>
    </div>
  {/each}
</div>

<style>
  .toast-text {
    margin: 0;
    padding-top: var(--sp-1);
    color: var(--text);
    overflow-wrap: anywhere;
  }

  .toast-close {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: var(--hit-min);
    height: var(--hit-min);
    padding: 0;
    color: var(--text-2);
    background: transparent;
    border: 0;
    border-radius: var(--r-sm);
    cursor: pointer;
  }

  .toast-close:hover {
    color: var(--text);
    background: var(--bg-4);
  }
</style>
