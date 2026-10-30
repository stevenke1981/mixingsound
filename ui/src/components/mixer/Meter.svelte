<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import { initialBallistics, stepBallistics, type BallisticsInput, type BallisticsState } from '../../lib/ballistics'
  import {
    dbToMeterPos,
    formatDbReadout,
    levelBand,
    linearToDb,
    litSegmentCount,
    meterSegmentCount,
    segmentDb,
    type LevelBand,
  } from '../../lib/dbScale'
  import { meterBank, type MeterFrame, type MeterKey } from '../../lib/meterBank'

  interface Props {
    /** Track id, or 'master'. */
    meterKey: MeterKey
    /** Track or master name, for the accessible label. */
    name: string
    /** Master variant: wider channels, taller clip LED. */
    wide?: boolean
  }
  let { meterKey, name, wide = false }: Props = $props()

  /** Peak readout and aria label refresh at most twice a second (§7). */
  const READOUT_INTERVAL_MS = 500
  const CHANNEL_GAP_PX = 2
  const RMS_BAR_PX = 2
  const HOLD_TICK_PX = 2
  const RMS_ALPHA = 0.55

  interface Palette {
    ok: string
    warn: string
    hot: string
    clip: string
    off: string
    peak: string
    text: string
    segH: number
    segGap: number
  }

  let canvas: HTMLCanvasElement | undefined = $state()
  /** Peak-hold text, null until the first readout tick. */
  let peakText: string | null = $state(null)

  const latched = $derived(Boolean(store.clipLatch[meterKey]))
  const ariaLabel = $derived(
    peakText === null ? `${name} level` : `${name} level, peak ${peakText} dB`,
  )

  // Plain (non-reactive) state: written by the shared animation loop, read by draw().
  let palette: Palette | null = null
  let left: BallisticsState = initialBallistics()
  let right: BallisticsState = initialBallistics()
  let lastReadoutMs = Number.NEGATIVE_INFINITY

  $effect(() => {
    const key = meterKey
    palette = readPalette()
    left = initialBallistics()
    right = initialBallistics()
    return meterBank.register({ key, frame: onFrame })
  })

  /** Reads the meter tokens once; canvas cannot use CSS variables directly. */
  function readPalette(): Palette {
    const styles = getComputedStyle(document.documentElement)
    const token = (variable: string): string => styles.getPropertyValue(variable).trim()
    return {
      ok: token('--meter-ok'),
      warn: token('--meter-warn'),
      hot: token('--meter-hot'),
      clip: token('--meter-clip'),
      off: token('--meter-off'),
      peak: token('--meter-peak'),
      text: token('--text'),
      segH: Number.parseFloat(token('--meter-seg-h')) || 3,
      segGap: Number.parseFloat(token('--meter-seg-gap')) || 1,
    }
  }

  function channelInput(peakLinear: number, rmsLinear: number): BallisticsInput {
    return { peakDb: linearToDb(peakLinear), rmsDb: linearToDb(rmsLinear), peakLinear }
  }

  function onFrame(frame: MeterFrame): void {
    const level = frame.level
    left = stepBallistics(left, channelInput(level?.peakL ?? 0, level?.rmsL ?? 0), frame.dtSeconds)
    right = stepBallistics(right, channelInput(level?.peakR ?? 0, level?.rmsR ?? 0), frame.dtSeconds)
    draw()
    if (frame.nowMs - lastReadoutMs >= READOUT_INTERVAL_MS) {
      lastReadoutMs = frame.nowMs
      peakText = formatDbReadout(Math.max(left.peak, right.peak))
    }
  }

  function bandColour(band: LevelBand, pal: Palette): string {
    switch (band) {
      case 'ok':
        return pal.ok
      case 'warn':
        return pal.warn
      case 'hot':
        return pal.hot
      case 'clip':
        return pal.clip
    }
  }

  function draw(): void {
    const target = canvas
    const pal = palette
    if (!target || !pal) return
    const ctx = target.getContext('2d')
    if (!ctx) return

    const dpr = window.devicePixelRatio || 1
    const w = target.clientWidth
    const h = target.clientHeight
    const backingW = Math.round(w * dpr)
    const backingH = Math.round(h * dpr)
    if (target.width !== backingW || target.height !== backingH) {
      target.width = backingW
      target.height = backingH
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    ctx.clearRect(0, 0, w, h)
    if (w <= CHANNEL_GAP_PX || h <= 0) return

    const pitch = pal.segH + pal.segGap
    const count = meterSegmentCount(h, pitch)
    const channelW = (w - CHANNEL_GAP_PX) / 2
    const ladderH = count * pitch
    const colours: string[] = []
    for (let i = 0; i < count; i++) colours.push(bandColour(levelBand(segmentDb(i, count)), pal))

    drawChannel(ctx, 0, channelW, h, { count, pitch, ladderH, colours }, pal, left)
    drawChannel(ctx, channelW + CHANNEL_GAP_PX, channelW, h, { count, pitch, ladderH, colours }, pal, right)
  }

  interface Ladder {
    count: number
    pitch: number
    ladderH: number
    colours: string[]
  }

  /** One channel: lit LED segments to the peak, RMS bar at centre, peak-hold tick. Floor draws nothing. */
  function drawChannel(
    ctx: CanvasRenderingContext2D,
    x: number,
    channelW: number,
    h: number,
    ladder: Ladder,
    pal: Palette,
    state: BallisticsState,
  ): void {
    const lit = litSegmentCount(dbToMeterPos(state.peak), ladder.count)
    for (let i = 0; i < ladder.count; i++) {
      ctx.fillStyle = i < lit ? ladder.colours[i] : pal.off
      ctx.fillRect(x, h - (i + 1) * ladder.pitch, channelW, pal.segH)
    }

    const rmsPos = dbToMeterPos(state.rms)
    if (rmsPos > 0) {
      const barH = rmsPos * ladder.ladderH
      ctx.globalAlpha = RMS_ALPHA
      ctx.fillStyle = pal.text
      ctx.fillRect(x + (channelW - RMS_BAR_PX) / 2, h - barH, RMS_BAR_PX, barH)
      ctx.globalAlpha = 1
    }

    const holdPos = dbToMeterPos(state.hold)
    if (holdPos > 0) {
      ctx.fillStyle = pal.peak
      ctx.fillRect(x, h - holdPos * ladder.ladderH - HOLD_TICK_PX / 2, channelW, HOLD_TICK_PX)
    }
  }
</script>

<div class="meter" class:is-wide={wide}>
  <span class="peak mono">{peakText ?? '−∞'}</span>
  <button
    type="button"
    class="clip-led"
    class:is-latched={latched}
    aria-label="Clip {name}: {latched ? 'latched, activate to clear' : 'clear'}"
    title="Clip indicator: click to clear"
    onclick={() => store.clearClip(meterKey)}
  ></button>
  <div class="meter-canvas" role="img" aria-label={ariaLabel}>
    <canvas bind:this={canvas} aria-hidden="true"></canvas>
  </div>
</div>

<style>
  .meter {
    display: flex;
    flex: none;
    flex-direction: column;
    align-items: center;
    align-self: stretch;
    gap: var(--sp-2);
    width: 30px;
    min-height: 0;
  }

  .meter.is-wide {
    --meter-w: 12px;
    width: 34px;
  }

  .peak {
    width: 30px;
    overflow: hidden;
    color: var(--text-2);
    font: var(--fw-medium) var(--fs-xs) / 1 var(--font-mono);
    text-align: center;
    white-space: nowrap;
  }

  .clip-led {
    flex: none;
    width: calc(var(--meter-w) * 2 + 2px);
    height: 6px;
    padding: 0;
    background: var(--meter-off);
    border: 1px solid var(--line-strong);
    border-radius: var(--r-xs);
  }

  .meter.is-wide .clip-led {
    height: 8px;
  }

  .clip-led.is-latched {
    background: var(--meter-clip);
    border-color: var(--meter-clip);
    box-shadow: var(--shadow-1);
  }

  .meter-canvas {
    flex: 1 1 auto;
    width: calc(var(--meter-w) * 2 + 2px);
    min-height: 0;
    background: var(--well);
    border-radius: var(--r-xs);
    box-shadow: var(--shadow-inset);
  }

  .meter-canvas canvas {
    width: 100%;
    height: 100%;
  }
</style>
