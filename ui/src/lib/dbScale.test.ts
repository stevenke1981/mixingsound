import { describe, expect, it } from 'vitest'
import {
  BAND_CLIP_DB,
  BAND_HOT_DB,
  BAND_WARN_DB,
  DB_FLOOR,
  dbToFaderPos,
  dbToMeterPos,
  faderKeyDb,
  faderPosToDb,
  faderScaleLayout,
  formatDbReadout,
  levelBand,
  linearToDb,
  litSegmentCount,
  meterPosToDb,
  meterSegmentCount,
  parseDbInput,
  scaleLabelText,
  segmentDb,
  clampGainDb,
} from './dbScale'
import { MAX_TRACK_GAIN_DB, MIN_GAIN_DB } from './types'

/** Spec §4.1 fader law: [dB, position]. */
const FADER_TABLE: readonly [number, number][] = [
  [12, 1.0],
  [6, 0.875],
  [0, 0.75],
  [-10, 0.58],
  [-20, 0.42],
  [-30, 0.29],
  [-40, 0.19],
  [-60, 0.07],
  [-96, 0.0],
]

/** Spec §4.2 meter law: [dBFS, position]. */
const METER_TABLE: readonly [number, number][] = [
  [-60, 0],
  [-40, 0.12],
  [-30, 0.24],
  [-20, 0.42],
  [-10, 0.64],
  [-6, 0.76],
  [0, 0.92],
  [3, 1.0],
]

const EPS = 1e-9

describe('faderPosToDb (spec 4.1 table)', () => {
  it.each(FADER_TABLE.filter(([, pos]) => pos > 0.005))(
    'returns %s dB at the spec table point',
    (db, pos) => {
      expect(faderPosToDb(pos)).toBeCloseTo(db, 9)
    },
  )

  it('returns −96 dB (−∞) at the bottom and at positions ≤ 0.005', () => {
    expect(faderPosToDb(0)).toBe(MIN_GAIN_DB)
    expect(faderPosToDb(0.005)).toBe(MIN_GAIN_DB)
  })

  it('interpolates linearly between table points', () => {
    expect(faderPosToDb(0.5)).toBeCloseTo(-15, 9)
  })

  it('clamps positions above 1 to +12 dB', () => {
    expect(faderPosToDb(1.5)).toBe(12)
  })

  it('treats NaN as −∞ instead of throwing', () => {
    expect(faderPosToDb(Number.NaN)).toBe(MIN_GAIN_DB)
  })
})

describe('dbToFaderPos (spec 4.1 table)', () => {
  it.each(FADER_TABLE)('maps %s dB to position %s', (db, pos) => {
    expect(dbToFaderPos(db)).toBeCloseTo(pos, 9)
  })

  it('returns 0 for −90 dB and below, including −∞', () => {
    expect(dbToFaderPos(-90)).toBe(0)
    expect(dbToFaderPos(-95)).toBe(0)
    expect(dbToFaderPos(Number.NEGATIVE_INFINITY)).toBe(0)
  })

  it('returns 0 for NaN', () => {
    expect(dbToFaderPos(Number.NaN)).toBe(0)
  })

  it('clamps values above +12 dB to position 1', () => {
    expect(dbToFaderPos(20)).toBe(1)
  })
})

describe('fader law round trip', () => {
  it.each([0.05, 0.07, 0.2, 0.42, 0.5, 0.75, 0.9, 1])(
    'position %s survives position → dB → position',
    (pos) => {
      expect(dbToFaderPos(faderPosToDb(pos))).toBeCloseTo(pos, 9)
    },
  )

  it.each([-89, -60, -30, -6, -0.5, 0, 3, 12])(
    '%s dB survives dB → position → dB',
    (db) => {
      expect(faderPosToDb(dbToFaderPos(db))).toBeCloseTo(db, 9)
    },
  )
})

describe('dbToMeterPos (spec 4.2 table)', () => {
  it.each(METER_TABLE)('maps %s dBFS to position %s', (db, pos) => {
    expect(dbToMeterPos(db)).toBeCloseTo(pos, 9)
  })

  it('clamps anything below −60 dBFS to the bottom', () => {
    expect(dbToMeterPos(-120)).toBe(0)
    expect(dbToMeterPos(Number.NEGATIVE_INFINITY)).toBe(0)
  })

  it('clamps anything above +3 dBFS to the top', () => {
    expect(dbToMeterPos(12)).toBe(1)
  })

  it('interpolates linearly between table points', () => {
    expect(dbToMeterPos(-25)).toBeCloseTo(0.33, 9)
  })
})

describe('meter law round trip', () => {
  it.each([-55, -30, -18, -6, -1, 0, 2.5])('%s dBFS survives dB → position → dB', (db) => {
    expect(meterPosToDb(dbToMeterPos(db))).toBeCloseTo(db, 9)
  })

  it('maps position 0 to −60 dBFS and position 1 to +3 dBFS', () => {
    expect(meterPosToDb(0)).toBe(-60)
    expect(meterPosToDb(1)).toBe(3)
  })
})

describe('linearToDb', () => {
  it('maps unity gain to 0 dB', () => {
    expect(linearToDb(1)).toBeCloseTo(0, 12)
  })

  it('maps 0.5 to about −6.02 dB', () => {
    expect(linearToDb(0.5)).toBeCloseTo(-6.0206, 3)
  })

  it('returns the −120 dB floor at 1e-6 and below', () => {
    expect(linearToDb(1e-6)).toBe(DB_FLOOR)
    expect(linearToDb(0)).toBe(DB_FLOOR)
  })

  it('returns the floor for negative and NaN input rather than NaN', () => {
    expect(linearToDb(-0.5)).toBe(DB_FLOOR)
    expect(linearToDb(Number.NaN)).toBe(DB_FLOOR)
  })
})

describe('levelBand', () => {
  it('colours levels at or below −18 dBFS as ok', () => {
    expect(levelBand(-18)).toBe('ok')
    expect(levelBand(-60)).toBe('ok')
  })

  it('colours −17.9 dBFS up to −6 dBFS as warn', () => {
    expect(levelBand(-17.9)).toBe('warn')
    expect(levelBand(-6)).toBe('warn')
  })

  it('colours −5.9 dBFS up to −1 dBFS as hot', () => {
    expect(levelBand(-5.9)).toBe('hot')
    expect(levelBand(-1)).toBe('hot')
  })

  it('colours anything above −1 dBFS as clip', () => {
    expect(levelBand(-0.9)).toBe('clip')
    expect(levelBand(3)).toBe('clip')
  })

  it('keeps the band thresholds aligned with the tokens', () => {
    expect([BAND_WARN_DB, BAND_HOT_DB, BAND_CLIP_DB]).toEqual([-18, -6, -1])
  })

  it('treats NaN as ok rather than clip', () => {
    expect(levelBand(Number.NaN)).toBe('ok')
  })
})

describe('segments', () => {
  it('counts floor(height / 4) segments', () => {
    expect(meterSegmentCount(200)).toBe(50)
    expect(meterSegmentCount(199)).toBe(49)
  })

  it('returns zero segments for zero, negative or non-finite heights', () => {
    expect(meterSegmentCount(0)).toBe(0)
    expect(meterSegmentCount(-10)).toBe(0)
    expect(meterSegmentCount(Number.NaN)).toBe(0)
  })

  it('lights nothing at position 0', () => {
    expect(litSegmentCount(0, 10)).toBe(0)
  })

  it('lights every segment at position 1', () => {
    expect(litSegmentCount(1, 10)).toBe(10)
  })

  it('lights ceil(pos × count) segments', () => {
    expect(litSegmentCount(0.01, 10)).toBe(1)
    expect(litSegmentCount(0.5, 10)).toBe(5)
    expect(litSegmentCount(0.51, 10)).toBe(6)
  })

  it('does not light an extra segment because of float noise', () => {
    expect(litSegmentCount(0.3, 10)).toBe(3)
  })

  it('clamps positions above 1 and handles an empty meter', () => {
    expect(litSegmentCount(7, 10)).toBe(10)
    expect(litSegmentCount(0.5, 0)).toBe(0)
  })

  it('colours the bottom segment by its own dBFS, the top segment hottest', () => {
    const count = 10
    expect(segmentDb(0, count)).toBeLessThan(segmentDb(count - 1, count))
    expect(segmentDb(count - 1, count)).toBeGreaterThan(0)
    expect(levelBand(segmentDb(0, count))).toBe('ok')
  })
})

describe('faderScaleLayout', () => {
  it('lists every scale label top to bottom when the track is tall enough', () => {
    const labels = faderScaleLayout(400, 16)
    expect(labels.map((l) => l.text)).toEqual([
      '+12',
      '+6',
      '0',
      '−10',
      '−20',
      '−30',
      '−40',
      '−60',
      '−∞',
    ])
  })

  it('places the +12 label at the top centre of the cap and −∞ at the bottom', () => {
    const labels = faderScaleLayout(400, 16)
    expect(labels[0].y).toBeCloseTo(8, 9)
    expect(labels[labels.length - 1].y).toBeCloseTo(392, 9)
  })

  it('omits labels that would sit closer than 14 px to the previous one', () => {
    const labels = faderScaleLayout(120, 16, 14)
    for (let i = 1; i < labels.length; i++) {
      expect(labels[i].y - labels[i - 1].y).toBeGreaterThanOrEqual(14)
    }
    expect(labels.length).toBeLessThan(9)
  })

  it('keeps only the first label when the track has no room', () => {
    expect(faderScaleLayout(0, 16).map((l) => l.db)).toEqual([12])
  })

  it('flags only the 0 dB label as zero', () => {
    const zero = faderScaleLayout(400, 16).filter((l) => l.isZero)
    expect(zero.map((l) => l.db)).toEqual([0])
  })
})

describe('scaleLabelText', () => {
  it.each([
    [12, '+12'],
    [6, '+6'],
    [0, '0'],
    [-10, '−10'],
    [-96, '−∞'],
  ])('renders %s dB as %s', (db, text) => {
    expect(scaleLabelText(db)).toBe(text)
  })
})

describe('parseDbInput', () => {
  it.each([
    ['-6', -6],
    ['+3', 3],
    ['0', 0],
    ['-6.5db', -6.5],
    ['-6.5 dB', -6.5],
    ['−6', -6],
    ['12', 12],
    ['.5', 0.5],
    ['5.', 5],
  ])('parses %j as %s dB', (text, db) => {
    expect(parseDbInput(text)).toBe(db)
  })

  it.each(['-inf', '-INF', '-∞', '−∞', '-infinity'])(
    'parses %j as −∞ (MIN_GAIN_DB)',
    (text) => {
      expect(parseDbInput(text)).toBe(MIN_GAIN_DB)
    },
  )

  it.each(['', 'abc', '--6', '6-', 'inf', '+inf', '1e400', 'NaN', '-6 db x'])(
    'rejects %j as not a number',
    (text) => {
      expect(parseDbInput(text)).toBeNull()
    },
  )

  it('returns out-of-range values unclamped for the caller to clamp', () => {
    expect(parseDbInput('-200')).toBe(-200)
    expect(parseDbInput('40')).toBe(40)
  })
})

describe('clampGainDb', () => {
  it('keeps values inside −96…+12 unchanged', () => {
    expect(clampGainDb(-6)).toBe(-6)
  })

  it('clamps below −96 to −96 and above +12 to +12', () => {
    expect(clampGainDb(-200)).toBe(MIN_GAIN_DB)
    expect(clampGainDb(40)).toBe(MAX_TRACK_GAIN_DB)
  })

  it('treats NaN as −∞', () => {
    expect(clampGainDb(Number.NaN)).toBe(MIN_GAIN_DB)
  })

  it('clamps −∞ to −96', () => {
    expect(clampGainDb(Number.NEGATIVE_INFINITY)).toBe(MIN_GAIN_DB)
  })
})

describe('formatDbReadout', () => {
  it.each([
    [-6, '−6.0'],
    [3, '+3.0'],
    [0, '0.0'],
    [-0.04, '0.0'],
    [12, '+12.0'],
    [-12.25, '−12.2'],
    [-89.96, '−90.0'],
  ])('formats %s dB as %s', (db, text) => {
    expect(formatDbReadout(db)).toBe(text)
  })

  it('shows −∞ at −90 dB and below, and for −∞ or NaN', () => {
    expect(formatDbReadout(-90)).toBe('−∞')
    expect(formatDbReadout(-96)).toBe('−∞')
    expect(formatDbReadout(Number.NEGATIVE_INFINITY)).toBe('−∞')
    expect(formatDbReadout(Number.NaN)).toBe('−∞')
  })

  it('round-trips through parseDbInput to within the display precision', () => {
    for (const db of [-60, -18.5, -6, -1.2, 0, 3, 12]) {
      expect(Math.abs((parseDbInput(formatDbReadout(db)) ?? NaN) - db)).toBeLessThanOrEqual(0.05 + EPS)
    }
  })
})

describe('faderKeyDb', () => {
  it('raises by 0.5 dB on ArrowUp and ArrowRight', () => {
    expect(faderKeyDb(-6, 'ArrowUp', false)).toBe(-5.5)
    expect(faderKeyDb(-6, 'ArrowRight', false)).toBe(-5.5)
  })

  it('lowers by 0.5 dB on ArrowDown and ArrowLeft', () => {
    expect(faderKeyDb(-6, 'ArrowDown', false)).toBe(-6.5)
    expect(faderKeyDb(-6, 'ArrowLeft', false)).toBe(-6.5)
  })

  it('moves by 0.1 dB with Shift held', () => {
    expect(faderKeyDb(-6, 'ArrowUp', true)).toBeCloseTo(-5.9, 9)
    expect(faderKeyDb(-6, 'ArrowDown', true)).toBeCloseTo(-6.1, 9)
  })

  it('moves by 3 dB on PgUp and PgDn', () => {
    expect(faderKeyDb(0, 'PageUp', false)).toBe(3)
    expect(faderKeyDb(0, 'PageDown', false)).toBe(-3)
  })

  it('jumps to −∞ on Home and +12 on End', () => {
    expect(faderKeyDb(0, 'Home', false)).toBe(MIN_GAIN_DB)
    expect(faderKeyDb(0, 'End', false)).toBe(MAX_TRACK_GAIN_DB)
  })

  it('resets to 0 dB on the 0 key', () => {
    expect(faderKeyDb(-30, '0', false)).toBe(0)
  })

  it('clamps steps at the range ends', () => {
    expect(faderKeyDb(12, 'ArrowUp', false)).toBe(MAX_TRACK_GAIN_DB)
    expect(faderKeyDb(MIN_GAIN_DB, 'ArrowDown', false)).toBe(MIN_GAIN_DB)
    expect(faderKeyDb(11, 'PageUp', false)).toBe(MAX_TRACK_GAIN_DB)
  })

  it('returns null for keys the fader does not handle', () => {
    expect(faderKeyDb(0, 'a', false)).toBeNull()
    expect(faderKeyDb(0, ' ', false)).toBeNull()
  })
})
