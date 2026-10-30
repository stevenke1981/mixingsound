import { describe, expect, it } from 'vitest'
import {
  fileStem,
  formatDb,
  formatFrames,
  formatKhz,
  formatPan,
  formatTime,
  framesToMs,
  middleTruncate,
  msToFrames,
  parseDb,
  parseTime,
  peakTone,
} from './format'

const SR = 48000

describe('formatTime', () => {
  it('formats zero as 00:00.000', () => {
    // Arrange / Act / Assert
    expect(formatTime(0, SR)).toBe('00:00.000')
  })

  it('zero-pads minutes, seconds and milliseconds', () => {
    expect(formatTime(12.345 * SR, SR)).toBe('00:12.345')
  })

  it('rounds to the nearest millisecond', () => {
    // 1 frame at 48 kHz is 0.0208 ms, so 0.5 ms rounds up to 1 ms
    expect(formatTime(Math.round(0.0005 * SR), SR)).toBe('00:00.001')
  })

  it('switches to h:mm:ss.mmm at exactly one hour', () => {
    expect(formatTime(3600 * SR, SR)).toBe('1:00:00.000')
  })

  it('keeps minutes below one hour in the m:ss.mmm form', () => {
    expect(formatTime((59 * 60 + 59.999) * SR, SR)).toBe('59:59.999')
  })

  it('shows multi-digit hours without padding', () => {
    expect(formatTime((12 * 3600 + 3 * 60 + 4.5) * SR, SR)).toBe('12:03:04.500')
  })

  it('clamps negative and non-finite frames to zero', () => {
    expect(formatTime(-500, SR)).toBe('00:00.000')
    expect(formatTime(Number.NaN, SR)).toBe('00:00.000')
  })

  it('returns zero time for a non-positive sample rate', () => {
    expect(formatTime(10 * SR, 0)).toBe('00:00.000')
  })
})

describe('parseTime', () => {
  it('round-trips every whole-millisecond time back to the same frame count', () => {
    const samplesMs = [0, 1, 12345, 59999, 61000, 3599500, 3600000, 7322004]
    for (const ms of samplesMs) {
      const frames = (ms * SR) / 1000
      const text = formatTime(frames, SR)
      expect(parseTime(text, SR)).toBe(frames)
    }
  })

  it('accepts plain seconds', () => {
    expect(parseTime('12.5', SR)).toBe(12.5 * SR)
  })

  it('accepts hours, minutes and seconds', () => {
    expect(parseTime('1:02:03.004', SR)).toBe(Math.round((3723.004) * SR))
  })

  it('rejects text that is not a time', () => {
    expect(parseTime('', SR)).toBeNull()
    expect(parseTime('abc', SR)).toBeNull()
    expect(parseTime('-1', SR)).toBeNull()
    expect(parseTime('1:2:3:4', SR)).toBeNull()
  })

  it('rejects seconds of 60 or more when minutes are given', () => {
    expect(parseTime('01:75.000', SR)).toBeNull()
  })

  it('returns null for an invalid sample rate', () => {
    expect(parseTime('1.0', 0)).toBeNull()
  })
})

describe('formatDb', () => {
  it('shows one decimal with a U+2212 minus', () => {
    expect(formatDb(-6)).toBe('\u22126.0')
  })

  it('shows a plus sign above zero', () => {
    expect(formatDb(3)).toBe('+3.0')
  })

  it('shows zero without a sign', () => {
    expect(formatDb(0)).toBe('0.0')
    expect(formatDb(-0.04)).toBe('0.0')
  })

  it('shows minus infinity at or below -90 dB', () => {
    expect(formatDb(-90)).toBe('\u2212\u221e')
    expect(formatDb(-96)).toBe('\u2212\u221e')
    expect(formatDb(Number.NEGATIVE_INFINITY)).toBe('\u2212\u221e')
  })

  it('treats NaN as silence', () => {
    expect(formatDb(Number.NaN)).toBe('\u2212\u221e')
  })
})

describe('parseDb', () => {
  it('parses plain and signed numbers', () => {
    expect(parseDb('-6')).toBe(-6)
    expect(parseDb('+3')).toBe(3)
    expect(parseDb('0')).toBe(0)
  })

  it('accepts the U+2212 minus and a dB suffix', () => {
    expect(parseDb('\u22126.5')).toBe(-6.5)
    expect(parseDb('-6.5db')).toBe(-6.5)
    expect(parseDb('-6.5 dB')).toBe(-6.5)
  })

  it('maps negative infinity to the minimum gain', () => {
    expect(parseDb('-inf')).toBe(-96)
    expect(parseDb('\u2212\u221e')).toBe(-96)
    expect(parseDb('-\u221e dB')).toBe(-96)
  })

  it('rejects text that is not a number', () => {
    expect(parseDb('')).toBeNull()
    expect(parseDb('loud')).toBeNull()
    expect(parseDb('inf')).toBeNull()
    expect(parseDb('1e3')).toBeNull()
  })

  it('round-trips formatDb for every value above the silence floor', () => {
    for (let db = -89.9; db <= 12; db += 0.7) {
      const rounded = Math.round(db * 10) / 10
      expect(parseDb(formatDb(db))).toBeCloseTo(rounded, 10)
    }
  })
})

describe('formatPan', () => {
  it('shows C at centre', () => {
    expect(formatPan(0)).toBe('C')
  })

  it('shows C for a pan too small to round to a step', () => {
    expect(formatPan(0.004)).toBe('C')
  })

  it('shows L with the rounded percentage', () => {
    expect(formatPan(-0.32)).toBe('L 32')
  })

  it('shows R with the rounded percentage', () => {
    expect(formatPan(0.18)).toBe('R 18')
  })

  it('clamps out-of-range pan to hard left and right', () => {
    expect(formatPan(-2)).toBe('L 100')
    expect(formatPan(2)).toBe('R 100')
  })
})

describe('formatFrames', () => {
  it('groups thousands with commas', () => {
    expect(formatFrames(1234567)).toBe('1,234,567')
  })

  it('leaves short numbers unchanged', () => {
    expect(formatFrames(999)).toBe('999')
  })

  it('keeps the sign of negative values', () => {
    expect(formatFrames(-1234)).toBe('-1,234')
  })
})

describe('sample-rate and unit helpers', () => {
  it('formats the sample rate in kHz', () => {
    expect(formatKhz(48000)).toBe('48')
    expect(formatKhz(44100)).toBe('44.1')
  })

  it('converts frames to milliseconds and back', () => {
    expect(framesToMs(4800, SR)).toBe(100)
    expect(msToFrames(100, SR)).toBe(4800)
  })
})

describe('peakTone', () => {
  it('is ok at or below -1 dBFS', () => {
    expect(peakTone(-1)).toBe('ok')
    expect(peakTone(-1.8)).toBe('ok')
  })

  it('is warn between -1 and 0 dBFS', () => {
    expect(peakTone(-0.5)).toBe('warn')
    expect(peakTone(0)).toBe('warn')
  })

  it('is clip above 0 dBFS', () => {
    expect(peakTone(0.1)).toBe('clip')
  })
})

describe('middleTruncate', () => {
  it('returns short text unchanged', () => {
    expect(middleTruncate('mix.wav', 20)).toBe('mix.wav')
  })

  it('keeps both ends and inserts an ellipsis', () => {
    const result = middleTruncate('C:/very/long/path/to/exports/final-mix.wav', 20)
    expect(result).toHaveLength(20)
    expect(result.startsWith('C:/very/lo')).toBe(true)
    expect(result.endsWith('mix.wav')).toBe(true)
    expect(result).toContain('\u2026')
  })

  it('returns the input when the limit is too small to abbreviate', () => {
    expect(middleTruncate('abcdef', 2)).toBe('abcdef')
  })
})

describe('fileStem', () => {
  it('returns the name without directory or extension', () => {
    expect(fileStem('C:\\Users\\eda\\My Mix.msproj')).toBe('My Mix')
  })

  it('handles forward slashes and names without extensions', () => {
    expect(fileStem('/home/eda/session')).toBe('session')
  })

  it('returns an empty string for an empty path', () => {
    expect(fileStem('')).toBe('')
  })
})
