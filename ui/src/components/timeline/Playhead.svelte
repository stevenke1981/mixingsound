<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import { frameToX } from '../../lib/geometry'

  /** `line` is the full-height cursor over the lanes; `head` is the triangle on the ruler. */
  interface Props {
    part?: 'line' | 'head'
  }
  let { part = 'line' }: Props = $props()

  const HEAD_HALF_PX = 5.5

  let el: HTMLDivElement | undefined = $state()

  $effect(() => {
    const node = el
    if (!node) return
    const offset = part === 'head' ? -HEAD_HALF_PX : 0
    let frame = 0
    let lastX = Number.NaN
    let shown: boolean | null = null

    const tick = (): void => {
      frame = requestAnimationFrame(tick)
      const v = store.view
      const x = frameToX(store.position, v.scrollFrames, v.fpp)
      const visible = x >= 0 && x <= v.viewportW
      if (visible !== shown) {
        node.hidden = !visible
        shown = visible
      }
      if (x !== lastX) {
        node.style.transform = `translateX(${x + offset}px)`
        lastX = x
      }
    }
    frame = requestAnimationFrame(tick)
    return () => cancelAnimationFrame(frame)
  })
</script>

<div class="playhead {part}" bind:this={el} aria-hidden="true"></div>

<style>
  .playhead {
    position: absolute;
    top: 0;
    left: 0;
    pointer-events: none;
    z-index: var(--z-overlay);
    will-change: transform;
  }

  .playhead.line {
    bottom: 0;
    width: 1px;
    background: var(--select);
  }

  .playhead.head {
    width: 11px;
    height: 9px;
    background: var(--select);
    clip-path: polygon(0 0, 100% 0, 50% 100%);
  }
</style>
