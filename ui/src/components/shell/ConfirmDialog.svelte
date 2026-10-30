<script lang="ts">
  import { tick } from 'svelte'
  import { trapTab } from './focusTrap'
  import { store } from '../../lib/store.svelte'

  let dialog = $state<HTMLElement | undefined>(undefined)
  let opener: HTMLElement | null = null
  const request = $derived(store.confirmRequest?.request ?? null)
  const open = $derived(request !== null)

  $effect(() => {
    if (!open) {
      opener?.focus()
      opener = null
      return
    }
    if (opener === null && document.activeElement instanceof HTMLElement) {
      opener = document.activeElement
    }
    void tick().then(() => {
      const primary = dialog?.querySelector<HTMLElement>('.btn-primary')
      const first = dialog?.querySelector<HTMLElement>('button')
      const target = primary ?? first ?? dialog
      target?.focus()
    })
  })

  function onKeydown(event: KeyboardEvent): void {
    if (dialog) trapTab(event, dialog)
  }

  function buttonClass(tone: 'normal' | 'primary' | 'danger' | undefined): string {
    if (tone === 'primary') return 'btn btn-primary'
    if (tone === 'danger') return 'btn btn-danger'
    return 'btn'
  }
</script>

{#if request}
  <div class="backdrop">
    <div
      class="dialog confirm-dialog"
      role="alertdialog"
      aria-modal="true"
      aria-labelledby="confirm-title"
      aria-describedby="confirm-message"
      tabindex="-1"
      bind:this={dialog}
      onkeydown={onKeydown}
    >
      <h2 class="dialog-head" id="confirm-title">{request.title}</h2>
      <div class="dialog-body">
        <p id="confirm-message" class="message">{request.message}</p>
      </div>
      <div class="dialog-foot">
        {#each request.buttons as button (button.id)}
          <button type="button" class={buttonClass(button.tone)} onclick={() => store.resolveConfirm(button.id)}>
            {button.label}
          </button>
        {/each}
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-head {
    color: var(--text);
  }

  .message {
    color: var(--text-2);
    line-height: var(--lh-base);
  }
</style>
