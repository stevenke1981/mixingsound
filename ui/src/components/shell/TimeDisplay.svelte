<script lang="ts">
  import { formatFrames, formatKhz, formatTime, parseTime } from '../../lib/format'
  import { store } from '../../lib/store.svelte'

  interface Props {
    frames: number
    sampleRate: number
  }

  let { frames, sampleRate }: Props = $props()

  let editing = $state(false)
  let draft = $state('')
  let input = $state<HTMLInputElement | undefined>(undefined)
  const display = $derived(formatTime(frames, sampleRate))
  const tooltip = $derived(`Frame ${formatFrames(frames)} @ ${formatKhz(sampleRate)} kHz`)

  $effect(() => {
    if (editing && input) {
      input.focus()
      input.select()
    }
  })

  function startEdit(): void {
    draft = display
    editing = true
  }

  function cancelEdit(): void {
    editing = false
  }

  function commitEdit(): void {
    const target = parseTime(draft, sampleRate)
    if (target === null) {
      store.toast('warn', 'Enter a time like 01:23.456')
      return
    }
    editing = false
    void store.seek(target)
  }

  function onInputKey(event: KeyboardEvent): void {
    if (event.key === 'Enter') {
      event.preventDefault()
      event.stopPropagation()
      commitEdit()
    } else if (event.key === 'Escape') {
      event.preventDefault()
      event.stopPropagation()
      cancelEdit()
    }
  }

  function onDisplayKey(event: KeyboardEvent): void {
    if (event.key === 'F2') startEdit()
  }
</script>

{#if editing}
  <input
    class="field mono time-input"
    bind:this={input}
    bind:value={draft}
    aria-label="Playhead time, mm:ss.mmm"
    spellcheck="false"
    onkeydown={onInputKey}
    onblur={cancelEdit}
  />
{:else}
  <button
    type="button"
    class="lcd"
    title={tooltip}
    aria-label={`Position ${display}. Double-click to type a time`}
    ondblclick={startEdit}
    onkeydown={onDisplayKey}
  >{display}</button>
{/if}

<style>
  .lcd {
    min-width: 152px;
    justify-content: center;
    cursor: text;
  }

  .time-input {
    width: 152px;
    height: 36px;
    font-size: var(--fs-md);
    text-align: center;
  }
</style>
