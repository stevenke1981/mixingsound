<script lang="ts" module>
  export type IconName =
    | 'new' | 'open' | 'save' | 'import' | 'export' | 'undo' | 'redo' | 'return'
    | 'play' | 'pause' | 'stop' | 'loop' | 'zoom-in' | 'zoom-out' | 'fit' | 'plus'
    | 'more' | 'close' | 'warn' | 'mixer' | 'check'

  // 24×24 stroke paths. STUB set by the controller; WP-A may refine the artwork, not the API.
  const PATHS: Record<IconName, string> = {
    new: 'M6 3h8l4 4v14H6z M14 3v4h4',
    open: 'M3 7h6l2 2h10v10H3z',
    save: 'M5 3h12l2 2v16H5z M8 3v6h8V3 M8 21v-7h8v7',
    import: 'M12 3v12 M7 10l5 5 5-5 M4 21h16',
    export: 'M12 15V3 M7 8l5-5 5 5 M4 21h16',
    undo: 'M9 14L4 9l5-5 M4 9h10a6 6 0 0 1 0 12h-3',
    redo: 'M15 14l5-5-5-5 M20 9H10a6 6 0 0 0 0 12h3',
    return: 'M6 5v14 M18 5L9 12l9 7z',
    play: 'M7 4l13 8-13 8z',
    pause: 'M7 4h3v16H7z M14 4h3v16h-3z',
    stop: 'M6 6h12v12H6z',
    loop: 'M17 2l3 3-3 3 M4 11V9a4 4 0 0 1 4-4h12 M7 22l-3-3 3-3 M20 13v2a4 4 0 0 1-4 4H4',
    'zoom-in': 'M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M21 21l-5-5 M11 8v6 M8 11h6',
    'zoom-out': 'M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14z M21 21l-5-5 M8 11h6',
    fit: 'M4 9V4h5 M20 9V4h-5 M4 15v5h5 M20 15v5h-5',
    plus: 'M12 5v14 M5 12h14',
    more: 'M5 12h.01 M12 12h.01 M19 12h.01',
    close: 'M6 6l12 12 M18 6L6 18',
    warn: 'M12 3l10 18H2z M12 10v5 M12 18h.01',
    mixer: 'M6 4v16 M12 4v16 M18 4v16 M4 14h4 M10 8h4 M16 16h4',
    check: 'M5 12l5 5 9-10',
  }
</script>

<script lang="ts">
  interface Props {
    name: IconName
    size?: number
  }
  let { name, size = 18 }: Props = $props()
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.75"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
  focusable="false"
>
  <path d={PATHS[name]} />
</svg>
