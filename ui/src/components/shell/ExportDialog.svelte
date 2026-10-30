<script lang="ts">
  import { tick } from 'svelte'
  import Icon from './Icon.svelte'
  import { trapTab } from './focusTrap'
  import { store } from '../../lib/store.svelte'
  import {
    formatDb,
    formatFrames,
    formatKhz,
    formatTime,
    middleTruncate,
    peakTone,
  } from '../../lib/format'
  import type { BitDepth } from '../../lib/types'

  type RangeChoice = 'project' | 'loop'

  const PATH_MAX_CHARS = 56
  const FOCUS_TARGET = '[data-autofocus]'

  let range = $state<RangeChoice>('project')
  let bitDepth = $state<BitDepth>('int24')
  /** null = follow the bit-depth default (on for 16-bit, off otherwise). */
  let ditherChoice = $state<boolean | null>(null)
  let dialog = $state<HTMLElement | undefined>(undefined)
  let opener: HTMLElement | null = null

  const ui = $derived(store.export)
  const open = $derived(store.export.open)
  const sr = $derived(store.sampleRate)
  const hasLoop = $derived(store.loop !== null)
  const activeRange: RangeChoice = $derived(range === 'loop' && hasLoop ? 'loop' : 'project')
  const span = $derived(
    activeRange === 'loop' && store.loop ? store.loop.end - store.loop.start : store.projectEnd,
  )
  const ditherAllowed = $derived(bitDepth !== 'float32')
  const dither = $derived(ditherAllowed && (ditherChoice ?? bitDepth === 'int16'))
  const progressPct = $derived(Math.round(ui.progress * 100))
  const title = $derived(
    ui.phase === 'running'
      ? 'Exporting…'
      : ui.phase === 'done'
        ? 'Export complete'
        : ui.phase === 'error'
          ? 'Export failed'
          : 'Export audio',
  )

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
      const target = dialog?.querySelector<HTMLElement>(FOCUS_TARGET) ?? dialog
      target?.focus()
    })
  })

  function onKeydown(event: KeyboardEvent): void {
    if (dialog) trapTab(event, dialog)
  }

  function runExport(): void {
    void store.runExport({ range: activeRange, bitDepth, dither })
  }
</script>

{#if ui.open}
  <div class="backdrop">
    <div
      class="dialog export-dialog"
      role="dialog"
      aria-modal="true"
      aria-labelledby="export-title"
      tabindex="-1"
      bind:this={dialog}
      onkeydown={onKeydown}
    >
      <h2 class="dialog-head" id="export-title">{title}</h2>

      {#if ui.phase === 'form'}
        <div class="dialog-body">
          <div class="dialog-row">
            <span class="label-caps" id="range-label">Range</span>
            <div class="seg-group" role="radiogroup" aria-labelledby="range-label">
              <label class="seg">
                <input type="radio" name="export-range" value="project" bind:group={range} data-autofocus />
                <span>Whole project</span>
              </label>
              <label class="seg">
                <input type="radio" name="export-range" value="loop" bind:group={range} disabled={!hasLoop} />
                <span>Loop region</span>
              </label>
            </div>
            {#if !hasLoop}
              <p class="hint">Set a loop region on the ruler to export it.</p>
            {/if}
          </div>

          <div class="dialog-row">
            <span class="label-caps" id="depth-label">Bit depth</span>
            <div class="seg-group" role="radiogroup" aria-labelledby="depth-label">
              <label class="seg">
                <input type="radio" name="export-depth" value="int16" bind:group={bitDepth} />
                <span>16-bit</span>
              </label>
              <label class="seg">
                <input type="radio" name="export-depth" value="int24" bind:group={bitDepth} />
                <span>24-bit</span>
              </label>
              <label class="seg">
                <input type="radio" name="export-depth" value="float32" bind:group={bitDepth} />
                <span>32-bit float</span>
              </label>
            </div>
          </div>

          <div class="dialog-row">
            <label class="check" class:is-disabled={!ditherAllowed}>
              <input
                type="checkbox"
                checked={dither}
                disabled={!ditherAllowed}
                onchange={(e) => (ditherChoice = e.currentTarget.checked)}
              />
              <span>Dither</span>
            </label>
            {#if !ditherAllowed}
              <p class="hint">Not applicable to float</p>
            {/if}
          </div>

          <p class="summary">
            Length <span class="mono">{formatTime(span, sr)}</span> · {formatKhz(sr)} kHz
          </p>
        </div>

        <div class="dialog-foot">
          <button type="button" class="btn" onclick={() => store.closeExport()}>Cancel</button>
          <button type="button" class="btn btn-primary" disabled={span <= 0} onclick={runExport}>Export…</button>
        </div>
      {:else if ui.phase === 'running'}
        <div class="dialog-body">
          <div class="progress" role="progressbar" aria-label="Export progress" aria-valuemin="0" aria-valuemax="100" aria-valuenow={progressPct}>
            <i style:--p={ui.progress}></i>
          </div>
          <p class="mono percent">{progressPct}%</p>
        </div>
        <div class="dialog-foot">
          <button type="button" class="btn btn-primary" disabled>Exporting…</button>
        </div>
      {:else if ui.phase === 'done' && ui.report}
        {@const report = ui.report}
        <div class="dialog-body">
          <p class="done-line"><Icon name="check" size={18} /> Export complete</p>
          <p class="path mono" title={report.path}>{middleTruncate(report.path, PATH_MAX_CHARS)}</p>
          <dl class="report">
            <dt>Length</dt>
            <dd class="mono">{formatTime(report.frames, sr)}</dd>
            <dt>Peak</dt>
            <dd class={`mono tone-${peakTone(report.peakDb)}`}>{formatDb(report.peakDb)} dBFS</dd>
            <dt>Clipped samples</dt>
            <dd class={`mono ${report.clippedSamples > 0 ? 'tone-clip' : ''}`}>{formatFrames(report.clippedSamples)}</dd>
          </dl>
          {#if report.clippedSamples > 0}
            <p class="clip-warn">
              {formatFrames(report.clippedSamples)} samples clipped — lower the master fader or export 32-bit float
            </p>
          {/if}
        </div>
        <div class="dialog-foot">
          <button type="button" class="btn" onclick={() => store.openExport()}>Export again</button>
          <button type="button" class="btn btn-primary" data-autofocus onclick={() => store.closeExport()}>Close</button>
        </div>
      {:else}
        <div class="dialog-body">
          <p class="error-text" role="alert">{ui.error ?? 'Export failed'}</p>
        </div>
        <div class="dialog-foot">
          <button type="button" class="btn" data-autofocus onclick={() => store.openExport()}>Back</button>
          <button type="button" class="btn btn-primary" onclick={() => store.closeExport()}>Close</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .dialog-head {
    color: var(--text);
  }

  .seg-group {
    display: inline-flex;
  }

  .hint {
    color: var(--text-muted);
    font-size: var(--fs-sm);
  }

  .check {
    display: inline-flex;
    align-items: center;
    gap: var(--sp-3);
    min-height: var(--hit-min);
    font-size: var(--fs-md);
  }

  .check.is-disabled {
    color: var(--text-disabled);
  }

  .summary {
    color: var(--text-2);
    font-size: var(--fs-sm);
  }

  .percent {
    color: var(--text-2);
    text-align: right;
  }

  .done-line {
    display: flex;
    align-items: center;
    gap: var(--sp-3);
    color: var(--meter-ok);
    font-weight: var(--fw-semibold);
  }

  .path {
    overflow: hidden;
    color: var(--text-2);
    white-space: nowrap;
  }

  .report {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: var(--sp-2) var(--sp-6);
    margin: 0;
    font-size: var(--fs-md);
  }

  .report dt {
    color: var(--text-muted);
  }

  .report dd {
    margin: 0;
    text-align: right;
  }

  .tone-ok {
    color: var(--meter-ok);
  }

  .tone-warn {
    color: var(--meter-warn);
  }

  .tone-clip {
    color: var(--meter-clip);
  }

  .clip-warn {
    color: var(--meter-clip);
    font-size: var(--fs-sm);
  }

  .error-text {
    color: var(--danger);
    white-space: pre-wrap;
  }
</style>
