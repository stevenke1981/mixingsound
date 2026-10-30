<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import TrackHeader from './TrackHeader.svelte'
  import Ruler from './Ruler.svelte'
  import LaneCanvas from './LaneCanvas.svelte'
  import Playhead from './Playhead.svelte'
  import HScroll from './HScroll.svelte'

  let bodyEl: HTMLDivElement | undefined = $state()
  let lanesEl: HTMLDivElement | undefined = $state()
  let viewH = $state(0)

  const tracks = $derived(store.project?.tracks ?? [])
  const selectedTrackId = $derived(store.selection.trackId)

  // Lane area size drives the canvas and the store's viewport (ui-spec §5.2).
  $effect(() => {
    const body = bodyEl
    if (!body) return
    const apply = (): void => {
      const w = lanesEl?.clientWidth ?? 0
      const h = body.clientHeight
      viewH = h
      store.setViewport(w, h)
    }
    const observer = new ResizeObserver(apply)
    observer.observe(body)
    apply()
    return () => observer.disconnect()
  })
</script>

<div class="timeline">
  <div class="tl-corner">
    <button type="button" class="btn tl-add" onclick={() => void store.addTrack()}>＋ Track</button>
  </div>

  <div class="tl-ruler">
    <Ruler />
  </div>

  <div id="timeline-body" class="tl-body" bind:this={bodyEl}>
    <div class="tl-headers">
      {#each tracks as track, i (track.id)}
        <TrackHeader {track} index={i} count={tracks.length} selected={selectedTrackId === track.id} />
      {/each}
      {#if tracks.length > 0}
        <div class="tl-add-row">
          <button type="button" class="btn tl-add" onclick={() => void store.addTrack()}>＋ Track</button>
        </div>
      {/if}
    </div>
    <div class="tl-lanes" bind:this={lanesEl}>
      <div class="tl-view" style:height="{viewH}px">
        <LaneCanvas scroller={bodyEl} />
        <Playhead part="line" />
      </div>
    </div>
  </div>

  <div class="tl-foot"></div>
  <div class="tl-hscroll">
    <HScroll />
  </div>
</div>

<style>
  .timeline {
    display: grid;
    grid-template-columns: var(--header-w) minmax(0, 1fr);
    grid-template-rows: var(--ruler-h) minmax(0, 1fr) 12px;
    width: 100%;
    height: 100%;
    min-width: 0;
    min-height: 0;
    background: var(--bg-0);
  }

  .tl-corner {
    display: flex;
    align-items: center;
    justify-content: center;
    min-width: 0;
    background: var(--bg-1);
    border-right: 1px solid var(--line);
    border-bottom: 1px solid var(--line);
  }

  .tl-add {
    min-width: var(--hit-min);
  }

  .tl-ruler {
    min-width: 0;
    background: var(--bg-1);
    border-bottom: 1px solid var(--line);
  }

  /* One vertical scroll container: headers and lanes are rows of the same grid row, so
     native scrolling keeps them aligned; the lane canvas sticks to the top of this box. */
  .tl-body {
    grid-column: 1 / -1;
    grid-row: 2;
    display: grid;
    grid-template-columns: var(--header-w) minmax(0, 1fr);
    align-items: stretch;
    min-width: 0;
    min-height: 0;
    overflow-x: hidden;
    overflow-y: auto;
  }

  .tl-headers {
    min-width: 0;
    background: var(--bg-2);
    border-right: 1px solid var(--line);
  }

  .tl-add-row {
    display: flex;
    align-items: center;
    justify-content: center;
    height: var(--lane-h);
    margin: var(--sp-3) var(--sp-4);
    border: 1px dashed var(--line-strong);
    border-radius: var(--r-sm);
  }

  .tl-lanes {
    position: relative;
    min-width: 0;
  }

  .tl-view {
    position: sticky;
    top: 0;
    overflow: hidden;
  }

  .tl-foot {
    background: var(--bg-1);
    border-right: 1px solid var(--line);
  }

  .tl-hscroll {
    min-width: 0;
  }
</style>
