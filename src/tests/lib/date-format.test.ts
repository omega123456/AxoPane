import {
  addMilliseconds,
  addMinutes,
  getTime,
  parseISO,
  subDays,
  subHours,
  subMinutes,
  subSeconds,
} from 'date-fns'
import { describe, expect, it } from 'vitest'
import {
  DATE_FORMATS,
  DEFAULT_DATE_FORMAT,
  dateFormatLabels,
  dateToneClassName,
  formatEntryDate,
  isDateFormat,
} from '@/lib/date-format'

// A fixed UTC reference; every absolute expectation below is the UTC rendering.
const SAMPLE = '2026-06-30T14:05:09Z'

function abs(
  value: string | null | undefined,
  format: Parameters<typeof formatEntryDate>[1]['format'],
  showTime = false,
  showSeconds = false,
) {
  return formatEntryDate(value, { format, showTime, showSeconds, relative: false })
}

describe('date-format', () => {
  it('exposes a default format that is part of the catalogue', () => {
    expect(DATE_FORMATS).toContain(DEFAULT_DATE_FORMAT)
    expect(DEFAULT_DATE_FORMAT).toBe('ymd')
  })

  it('validates format identifiers', () => {
    expect(isDateFormat('ymd')).toBe(true)
    expect(isDateFormat('dme')).toBe(true)
    expect(isDateFormat('dmy_his')).toBe(false)
    expect(isDateFormat('nonsense')).toBe(false)
    expect(isDateFormat('')).toBe(false)
  })

  it('has a label for every format', () => {
    for (const format of DATE_FORMATS) {
      expect(dateFormatLabels[format]).toBeTruthy()
    }
  })

  it('renders each absolute date format in UTC', () => {
    expect(abs(SAMPLE, 'ymd').text).toBe('2026-06-30')
    expect(abs(SAMPLE, 'dmy').text).toBe('30/06/2026')
    expect(abs(SAMPLE, 'mdy').text).toBe('06/30/2026')
    expect(abs(SAMPLE, 'med').text).toBe('Jun 30th 2026')
    expect(abs(SAMPLE, 'dme').text).toBe('30th Jun 2026')
    expect(abs(SAMPLE, 'lmd').text).toBe('June 30, 2026')
    expect(abs(SAMPLE, 'dml').text).toBe('30 June 2026')
    expect(abs(SAMPLE, 'dly').text).toBe('30th June 26')
    expect(abs('2026-06-10T00:00:00Z', 'dly').text).toBe('10th June 26')
    // Two-digit year zero-pads (year 2005 → "05").
    expect(abs('2005-03-01T00:00:00Z', 'dly').text).toBe('1st March 05')
  })

  it('appends HH:MM only when showTime is set, and seconds only with showSeconds', () => {
    expect(abs(SAMPLE, 'ymd', false).text).toBe('2026-06-30')
    expect(abs(SAMPLE, 'ymd', true).text).toBe('2026-06-30 14:05')
    expect(abs(SAMPLE, 'dmy', true).text).toBe('30/06/2026 14:05')
    expect(abs(SAMPLE, 'med', true).text).toBe('Jun 30th 2026 14:05')
    expect(abs(SAMPLE, 'ymd', true, true).text).toBe('2026-06-30 14:05:09')
    expect(abs(SAMPLE, 'dmy', true, true).text).toBe('30/06/2026 14:05:09')
    // Seconds are ignored when the time itself is hidden.
    expect(abs(SAMPLE, 'ymd', false, true).text).toBe('2026-06-30')
  })

  it('applies the correct ordinal suffix for named formats', () => {
    expect(abs('2026-06-01T00:00:00Z', 'dme').text).toBe('1st Jun 2026')
    expect(abs('2026-06-02T00:00:00Z', 'dme').text).toBe('2nd Jun 2026')
    expect(abs('2026-06-03T00:00:00Z', 'dme').text).toBe('3rd Jun 2026')
    expect(abs('2026-06-04T00:00:00Z', 'dme').text).toBe('4th Jun 2026')
    // The 11th–13th exception always takes "th".
    expect(abs('2026-06-11T00:00:00Z', 'dme').text).toBe('11th Jun 2026')
    expect(abs('2026-06-12T00:00:00Z', 'dme').text).toBe('12th Jun 2026')
    expect(abs('2026-06-13T00:00:00Z', 'dme').text).toBe('13th Jun 2026')
    expect(abs('2026-06-21T00:00:00Z', 'dme').text).toBe('21st Jun 2026')
    expect(abs('2026-01-04T00:00:00Z', 'dme').text).toBe('4th Jan 2026')
  })

  it('zero-pads numeric components', () => {
    expect(abs('2026-01-02T03:04:05Z', 'ymd', true, true).text).toBe('2026-01-02 03:04:05')
  })

  it('uses the default tone for absolute values', () => {
    expect(abs(SAMPLE, 'ymd').tone).toBe('default')
  })

  it('returns an em dash for missing or invalid input', () => {
    expect(abs(null, 'ymd')).toEqual({ text: '—', tone: 'default' })
    expect(abs(undefined, 'ymd')).toEqual({ text: '—', tone: 'default' })
    expect(
      formatEntryDate('', { format: 'ymd', showTime: false, showSeconds: false, relative: true }),
    ).toEqual({ text: '—', tone: 'default' })
    expect(
      formatEntryDate('not-a-date', {
        format: 'ymd',
        showTime: false,
        showSeconds: false,
        relative: true,
      }),
    ).toEqual({ text: '—', tone: 'default' })
  })

  describe('relative mode', () => {
    const nowDate = parseISO(SAMPLE)
    const now = getTime(nowDate)
    const rel = (value: string) =>
      formatEntryDate(value, {
        format: 'dmy',
        showTime: true,
        showSeconds: true,
        relative: true,
        now,
      })

    it('shows "just now" for sub-minute ages with the recent tone', () => {
      expect(rel(subSeconds(nowDate, 30).toISOString())).toEqual({
        text: 'just now',
        tone: 'recent',
      })
    })

    it('pluralises minutes and keeps the recent tone under an hour', () => {
      expect(rel(subMinutes(nowDate, 1).toISOString())).toEqual({
        text: '1 minute ago',
        tone: 'recent',
      })
      expect(rel(subMinutes(nowDate, 15).toISOString())).toEqual({
        text: '15 minutes ago',
        tone: 'recent',
      })
    })

    it('uses the today tone for hour-scale ages', () => {
      expect(rel(subHours(nowDate, 1).toISOString())).toEqual({
        text: '1 hour ago',
        tone: 'today',
      })
      expect(rel(subHours(nowDate, 2).toISOString())).toEqual({
        text: '2 hours ago',
        tone: 'today',
      })
    })

    it('uses the yesterday tone for 1-3 day ages before weekday labels take over', () => {
      expect(rel(subHours(nowDate, 25).toISOString())).toEqual({
        text: '1 day ago',
        tone: 'yesterday',
      })
      expect(rel(subDays(nowDate, 2).toISOString())).toEqual({
        text: '2 days ago',
        tone: 'yesterday',
      })
      expect(rel(subDays(nowDate, 3).toISOString())).toEqual({
        text: '3 days ago',
        tone: 'yesterday',
      })
    })

    it('shows weekday labels for older items from the past week and keeps the time when enabled', () => {
      expect(rel(subDays(nowDate, 4).toISOString())).toEqual({
        text: 'on Friday 14:05:09',
        tone: 'default',
      })
      expect(rel(subDays(nowDate, 6).toISOString())).toEqual({
        text: 'on Wednesday 14:05:09',
        tone: 'default',
      })
    })

    it('falls back to the absolute format (honouring showTime) at the 7-day cutoff', () => {
      const atCutoff = subDays(nowDate, 7).toISOString()
      expect(rel(atCutoff)).toEqual({ text: '23/06/2026 14:05:09', tone: 'default' })
    })

    it('keeps the 3-day label ahead of weekday formatting near the boundary', () => {
      const justUnderFourDays = addMilliseconds(subDays(nowDate, 4), 1).toISOString()
      expect(rel(justUnderFourDays)).toEqual({
        text: '3 days ago',
        tone: 'yesterday',
      })
    })

    it('uses weekday labels again immediately after the 3-day priority window', () => {
      const atFourDays = subDays(nowDate, 4).toISOString()
      expect(rel(atFourDays)).toEqual({ text: 'on Friday 14:05:09', tone: 'default' })
    })

    it('omits the time from weekday labels when showTime is disabled', () => {
      const value = subDays(nowDate, 4).toISOString()
      expect(
        formatEntryDate(value, {
          format: 'dmy',
          showTime: false,
          showSeconds: false,
          relative: true,
          now,
        }),
      ).toEqual({ text: 'on Friday', tone: 'default' })
    })

    it('clamps clock-skewed future timestamps to "just now"', () => {
      expect(rel(addMinutes(nowDate, 1).toISOString())).toEqual({
        text: 'just now',
        tone: 'recent',
      })
    })
  })

  it('maps every tone to a colour utility', () => {
    expect(dateToneClassName.recent).toContain('accent-green')
    expect(dateToneClassName.today).toContain('accent-blue')
    expect(dateToneClassName.yesterday).toContain('accent-amber')
    expect(dateToneClassName.default).toContain('text-light-text-muted')
  })

  describe('absolute-format cache', () => {
    it('returns the identical (referentially equal) result for a repeated value|pattern pair', () => {
      const first = abs(SAMPLE, 'ymd', true, true)
      const second = abs(SAMPLE, 'ymd', true, true)

      expect(second).toBe(first)
      expect(second).toEqual({ text: '2026-06-30 14:05:09', tone: 'default' })
    })

    it('keys the cache on both value and pattern — different formats never collide', () => {
      const ymd = abs(SAMPLE, 'ymd')
      const dmy = abs(SAMPLE, 'dmy')

      expect(ymd).not.toBe(dmy)
      expect(ymd.text).toBe('2026-06-30')
      expect(dmy.text).toBe('30/06/2026')

      // Re-requesting the first pairing still hits the cached entry.
      expect(abs(SAMPLE, 'ymd')).toBe(ymd)
    })

    it('keys the cache on the showTime/showSeconds-derived pattern, not just the format id', () => {
      const dateOnly = abs(SAMPLE, 'ymd', false)
      const withTime = abs(SAMPLE, 'ymd', true)

      expect(dateOnly).not.toBe(withTime)
      expect(dateOnly.text).toBe('2026-06-30')
      expect(withTime.text).toBe('2026-06-30 14:05')
    })

    it('bypasses the cache for relative-mode results within the cutoff', () => {
      const nowDate = parseISO(SAMPLE)
      const now = getTime(nowDate)
      const value = subMinutes(nowDate, 1).toISOString()
      const options = {
        format: 'ymd' as const,
        showTime: false,
        showSeconds: false,
        relative: true,
        now,
      }

      const first = formatEntryDate(value, options)
      const second = formatEntryDate(value, options)

      // Same inputs, but relative phrases are never cached — a fresh object
      // is produced every call since "now" is volatile.
      expect(second).not.toBe(first)
      expect(second).toEqual(first)
      expect(second).toEqual({ text: '1 minute ago', tone: 'recent' })
    })

    it('falls through to (and reuses) the cached absolute entry once relative mode is past the cutoff', () => {
      const nowDate = parseISO(SAMPLE)
      const now = getTime(nowDate)
      const value = subDays(nowDate, 8).toISOString()
      const relativeOptions = {
        format: 'ymd' as const,
        showTime: false,
        showSeconds: false,
        relative: true,
        now,
      }
      const absoluteOptions = {
        format: 'ymd' as const,
        showTime: false,
        showSeconds: false,
        relative: false,
      }

      const viaRelativeFallback = formatEntryDate(value, relativeOptions)
      const viaAbsolute = formatEntryDate(value, absoluteOptions)

      // Both paths compute the same `value|pattern` cache key (relative mode
      // no longer influences the key once it falls back to absolute), so the
      // second call reuses the first's cached result.
      expect(viaAbsolute).toBe(viaRelativeFallback)
    })
  })
})
