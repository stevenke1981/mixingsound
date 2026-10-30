<script lang="ts">
  import { store } from '../../lib/store.svelte'
  import Fader from './Fader.svelte'
  import Meter from './Meter.svelte'

  const masterDb = $derived(store.project?.master.gainDb ?? 0)
</script>

<div class="master" role="group" aria-label="Master">
  <span class="label-caps title">MASTER</span>
  <Fader
    value={masterDb}
    label="Master volume"
    onlive={(db) => store.setMasterGain(db)}
    onend={() => void store.endGesture()}
  >
    {#snippet meter()}
      <Meter meterKey="master" name="Master" wide />
    {/snippet}
  </Fader>
</div>

<style>
  .master {
    display: flex;
    box-sizing: border-box;
    flex-direction: column;
    gap: var(--sp-3);
    width: var(--master-w);
    height: 100%;
    padding: var(--sp-3);
    background: var(--bg-2);
  }

  .title {
    flex: none;
    height: 18px;
    line-height: 18px;
    text-align: center;
  }
</style>
