import { describe, expect, it } from 'vitest'
import {
  ellipsize,
  formatClock,
  formatDbShort,
  formatGainBadge,
  formatPan,
  formatSignedSeconds,
  formatTrackSummary,
  gestureBubble,
  mixHex,
  readLanePalette,
  type GhostState,
} from './drawClip'

const SR = 48000

function ghost(overrides: Partial<GhostState> = {}): GhostState {
  return { start: 0, offset: 0, length: SR, fadeIn: 0, fadeOut: 0, gainDb: 0, trackIndex: 0, ...overrides }
}

describe('formatDbShort', () => {
  it('uses the typographic minus for negative values', () => {
    expect(formatDbShort(-6)).toBe('−6.0')
  })

  it('adds an explicit plus sign for positive values', () => {
    expect(formatDbShort(3)).toBe('+3.0')
  })

  it('renders zero without a sign, including values that round to zero', () => {
    expect(formatDbShort(0)).toBe('0.0')
    expect(formatDbShort(-0.04)).toBe('0.0')
  })

  it('renders minus infinity at and below -90 dB and for NaN', () => {
    expect(formatDbShort(-90)).toBe('−∞')
    expect(formatDbShort(-120)).toBe('−∞')
    expect(formatDbShort(Number.NaN)).toBe('−∞')
  })
})

describe('formatGainBadge', () => {
  it('hides the badge when the clip gain is 0.0 dB', () => {
    expect(formatGainBadge(0)).toBeNull()
  })

  it('shows the signed gain with its unit when non-zero', () => {
    expect(formatGainBadge(3)).toBe('+3.0 dB')
  })
})

describe('formatPan', () => {
  it('shows centre for zero and tiny offsets', () => {
    expect(formatPan(0)).toBe('C')
    expect(formatPan(-0.004)).toBe('C')
  })

  it('shows the side and rounded percentage', () => {
    expect(formatPan(-0.32)).toBe('L 32')
    expect(formatPan(0.18)).toBe('R 18')
  })
})

describe('formatTrackSummary', () => {
  it('joins gain and pan with a middle dot', () => {
    expect(formatTrackSummary(-6, -0.32)).toBe('−6.0 dB · L 32')
  })
})

describe('formatClock', () => {
  it('formats below one hour as mm:ss.mmm', () => {
    expect(formatClock(12.345 * SR, SR)).toBe('00:12.345')
  })

  it('adds the hour field from one hour upward', () => {
    expect(formatClock(3661.5 * SR, SR)).toBe('1:01:01.500')
  })

  it('clamps negative frames and handles a zero sample rate', () => {
    expect(formatClock(-100, SR)).toBe('00:00.000')
    expect(formatClock(100, 0)).toBe('00:00.000')
  })
})

describe('formatSignedSeconds', () => {
  it('signs positive and negative deltas', () => {
    expect(formatSignedSeconds(0.25 * SR, SR)).toBe('+0.250 s')
    expect(formatSignedSeconds(-0.25 * SR, SR)).toBe('−0.250 s')
  })
})

describe('mixHex', () => {
  it('mixes a six-digit hex toward white', () => {
    expect(mixHex('#000000', 0.5)).toBe('rgb(128 128 128)')
    expect(mixHex('#ffffff', 0.5)).toBe('rgb(255 255 255)')
  })

  it('returns null for anything that is not six-digit hex', () => {
    expect(mixHex('rgb(1 2 3)', 0.5)).toBeNull()
    expect(mixHex('#abc', 0.5)).toBeNull()
  })
})

describe('ellipsize', () => {
  const measure = (text: string): number => text.length * 10

  it('returns the text unchanged when it fits', () => {
    expect(ellipsize(measure, 'Drums', 100)).toBe('Drums')
  })

  it('shortens with an ellipsis when it does not fit', () => {
    const out = ellipsize(measure, 'Vocals Lead', 60)
    expect(out.endsWith('…')).toBe(true)
    expect(measure(out)).toBeLessThanOrEqual(60)
  })

  it('returns an empty string when nothing fits', () => {
    expect(ellipsize(measure, 'Drums', 0)).toBe('')
    expect(ellipsize(measure, 'Drums', 5)).toBe('')
  })
})

describe('gestureBubble', () => {
  it('shows start and signed offset for a move', () => {
    const text = gestureBubble('move', ghost({ start: 12.345 * SR }), 12.095 * SR, SR)
    expect(text).toBe('Start 00:12.345 (+0.250 s)')
  })

  it('shows start and length for a trim', () => {
    const text = gestureBubble('trimL', ghost({ start: SR, length: 2 * SR }), 0, SR)
    expect(text).toBe('Start 00:01.000 · Length 00:02.000')
  })

  it('shows the fade length in milliseconds', () => {
    expect(gestureBubble('fadeIn', ghost({ fadeIn: 0.12 * SR }), 0, SR)).toBe('Fade in 120 ms')
    expect(gestureBubble('fadeOut', ghost({ fadeOut: 0.5 * SR }), 0, SR)).toBe('Fade out 500 ms')
  })

  it('shows the clip gain with its unit', () => {
    expect(gestureBubble('gain', ghost({ gainDb: 3 }), 0, SR)).toBe('+3.0 dB')
  })
})

describe('readLanePalette', () => {
  function fakeStyle(values: Record<string, string>): CSSStyleDeclaration {
    return {
      getPropertyValue: (name: string): string => values[name] ?? '',
    } as unknown as CSSStyleDeclaration
  }

  it('reads colour, font and size tokens with their trimmed values', () => {
    const pal = readLanePalette(
      fakeStyle({ '--accent': ' #e8a03c ', '--fs-xs': '11px', '--r-sm': '3px', '--font-label': "'Bahnschrift', sans-serif" }),
    )
    expect(pal.accent).toBe('#e8a03c')
    expect(pal.fsXs).toBe(11)
    expect(pal.radius).toBe(3)
    expect(pal.fontLabel).toBe("'Bahnschrift', sans-serif")
  })

  it('falls back to inert values when tokens are missing', () => {
    const pal = readLanePalette(fakeStyle({}))
    expect(pal.accent).toBe('transparent')
    expect(pal.fsXs).toBe(11)
    expect(pal.fontLabel).toBe('sans-serif')
  })
})
