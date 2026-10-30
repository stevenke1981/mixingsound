<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import { formatDb, formatTime, framesToMs, msToFrames, parseDb } from '../../lib/format'
  import { MAX_CLIP_GAIN_DB, MIN_GAIN_DB } from '../../lib/types'

  type FadeSide = 'in' | 'out'

  const GAIN_EPSILON_DB = 0.005

  const info = $derived(
    store.selection.clipId === null ? undefined : store.clipById(store.selection.clipId),
  )
  const sr = $derived(store.sampleRate)
  const source = $derived(info ? store.sourceById(info.clip.sourceId) : undefined)

  // Enter and Escape both end editing through blur, so each edit is committed exactly once.
  let cancelling = false

  function onFieldKey(event: KeyboardEvent): void {
    const field = event.currentTarget as HTMLInputElement
    if (event.key === 'Enter') {
      event.preventDefault()
      field.blur()
    } else if (event.key === 'Escape') {
      event.stopPropagation()
      cancelling = true
      field.blur()
    }
  }

  function onGainBlur(input: HTMLInputElement): void {
    const clip = info?.clip
    if (!clip) return
    if (cancelling) {
      cancelling = false
      input.value = formatDb(clip.gainDb)
      return
    }
    const db = parseDb(input.value)
    if (db === null) {
      store.toast('warn', 'Gain must be a number in dB, for example -6 or -inf')
      input.value = formatDb(clip.gainDb)
      return
    }
    const clamped = Math.min(MAX_CLIP_GAIN_DB, Math.max(MIN_GAIN_DB, db))
    input.value = formatDb(clamped)
    if (Math.abs(clamped - clip.gainDb) >= GAIN_EPSILON_DB) void store.setClipGain(clip.id, clamped)
  }

  function fadeText(side: FadeSide): string {
    const clip = info?.clip
    if (!clip) return ''
    const frames = side === 'in' ? clip.fadeIn : clip.fadeOut
    return String(Math.round(framesToMs(frames, sr)))
  }

  function onFadeBlur(input: HTMLInputElement, side: FadeSide): void {
    const clip = info?.clip
    if (!clip) return
    if (cancelling) {
      cancelling = false
      input.value = fadeText(side)
      return
    }
    const ms = Number(input.value)
    if (input.value.trim() === '' || !Number.isFinite(ms) || ms < 0) {
      store.toast('warn', 'Fade must be a time in milliseconds')
      input.value = fadeText(side)
      return
    }
    const wanted = Math.max(0, msToFrames(ms, sr))
    const fadeIn = side === 'in' ? Math.min(wanted, clip.length - clip.fadeOut) : clip.fadeIn
    const fadeOut = side === 'out' ? Math.min(wanted, clip.length - clip.fadeIn) : clip.fadeOut
    if (fadeIn === clip.fadeIn && fadeOut === clip.fadeOut) {
      input.value = fadeText(side)
      return
    }
    void store.setClipFades(clip.id, fadeIn, fadeOut)
  }

  function resetGain(): void {
    if (info) void store.setClipGain(info.clip.id, 0)
  }
</script>

{#if info}
  {@const clip = info.clip}
  <div class="inspector">
    <span class="source truncate" title={source?.path ?? ''}>{source?.name ?? 'Unknown source'}</span>

    <label class="item">
      <span class="label-caps">Gain</span>
      <input
        class="field num gain"
        value={formatDb(clip.gainDb)}
        aria-label="Clip gain in dB"
        spellcheck="false"
        onkeydown={onFieldKey}
        onblur={(e) => onGainBlur(e.currentTarget)}
      />
    </label>

    <label class="item">
      <span class="label-caps">Fade in</span>
      <input
        class="field num fade"
        value={fadeText('in')}
        aria-label="Fade in, milliseconds"
        onkeydown={onFieldKey}
        onblur={(e) => onFadeBlur(e.currentTarget, 'in')}
      />
    </label>

    <label class="item">
      <span class="label-caps">Fade out</span>
      <input
        class="field num fade"
        value={fadeText('out')}
        aria-label="Fade out, milliseconds"
        onkeydown={onFieldKey}
        onblur={(e) => onFadeBlur(e.currentTarget, 'out')}
      />
    </label>

    <span class="readout"><span class="label-caps">Start</span><span class="mono">{formatTime(clip.start, sr)}</span></span>
    <span class="readout"><span class="label-caps">Length</span><span class="mono">{formatTime(clip.length, sr)}</span></span>

    <button type="button" class="btn reset" aria-label="Reset clip gain to 0 dB" title="Reset clip gain to 0 dB" onclick={resetGain}>0 dB</button>
  </div>
{/if}

<style>
  .inspector {
    display: flex;
    align-items: center;
    gap: var(--sp-5);
    min-width: 0;
  }

  .source {
    max-width: 200px;
    color: var(--text);
    font-size: var(--fs-md);
  }

  .item {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-3);
  }

  .gain {
    width: 64px;
  }

  .fade {
    width: 56px;
  }

  .readout {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-3);
    color: var(--text-2);
    white-space: nowrap;
  }

  .reset {
    height: 24px;
    min-width: var(--hit-min);
    padding: 0 var(--sp-3);
    font-family: var(--font-mono);
  }
</style>
