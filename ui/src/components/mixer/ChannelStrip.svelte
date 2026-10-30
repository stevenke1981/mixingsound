<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import Fader from './Fader.svelte'
  import Knob from './Knob.svelte'
  import Meter from './Meter.svelte'

  interface Props {
    trackId: number
  }
  let { trackId }: Props = $props()

  const track = $derived(store.trackById(trackId))
  const isSelected = $derived(store.selection.trackId === trackId)
</script>

{#if track}
  <div
    class="strip"
    class:is-selected={isSelected}
    role="group"
    aria-label="Track {track.name}"
    style:--track-color={track.color}
  >
    <div class="bar" aria-hidden="true"></div>
    <button
      type="button"
      class="name truncate"
      title={track.name}
      onclick={() => store.selectTrack(trackId)}
    >{track.name}</button>
    <div class="row">
      <Knob
        value={track.pan}
        label="{track.name} pan"
        onlive={(pan) => store.setTrackPan(trackId, pan)}
        onend={() => void store.endGesture()}
      />
      <div class="ms-col">
        <button
          type="button"
          class="ms"
          class:is-mute={track.mute}
          aria-pressed={track.mute}
          aria-label="Mute {track.name}"
          title="Mute {track.name} (M)"
          onclick={() => void store.setTrackMute(trackId, !track.mute)}
        >M</button>
        <button
          type="button"
          class="ms"
          class:is-solo={track.solo}
          aria-pressed={track.solo}
          aria-label="Solo {track.name}"
          title="Solo {track.name} (S)"
          onclick={() => void store.setTrackSolo(trackId, !track.solo)}
        >S</button>
      </div>
    </div>
    <Fader
      value={track.gainDb}
      label="{track.name} volume"
      onlive={(db) => store.setTrackGain(trackId, db)}
      onend={() => void store.endGesture()}
    >
      {#snippet meter()}
        <Meter meterKey={trackId} name={track.name} />
      {/snippet}
    </Fader>
  </div>
{/if}

<style>
  .strip {
    display: flex;
    flex: none;
    flex-direction: column;
    gap: var(--sp-3);
    box-sizing: border-box;
    width: var(--strip-w);
    height: 100%;
    padding: var(--sp-3);
    background: var(--bg-2);
    border-right: 1px solid var(--line);
  }

  .strip.is-selected {
    background: var(--bg-3);
  }

  .bar {
    height: 3px;
    margin: 0 calc(var(--sp-3) * -1);
    background: var(--track-color);
  }

  .strip.is-selected .bar {
    height: 5px;
  }

  .name {
    flex: none;
    width: 100%;
    height: 18px;
    padding: 0;
    color: var(--text);
    background: none;
    border: 0;
    border-radius: var(--r-xs);
    font: var(--fw-medium) var(--fs-sm) / 18px var(--font-ui);
    text-align: center;
  }

  .name:hover {
    color: var(--accent-hi);
  }

  .row {
    display: flex;
    flex: none;
    align-items: center;
    justify-content: space-between;
    height: 38px;
  }

  .ms-col {
    display: grid;
    gap: var(--sp-2);
  }

  .ms-col .ms {
    height: 18px;
  }
</style>
