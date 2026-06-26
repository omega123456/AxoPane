import { describe, expect, it } from 'vitest'
import {
  DEFAULT_UPDATE_INTERVAL,
  UPDATE_INTERVAL_MS,
  UPDATE_INTERVAL_OPTIONS,
  isUpdateInterval,
} from '@/lib/update-intervals'

describe('update-intervals', () => {
  it('defaults to a daily cadence', () => {
    expect(DEFAULT_UPDATE_INTERVAL).toBe('1d')
    expect(isUpdateInterval(DEFAULT_UPDATE_INTERVAL)).toBe(true)
  })

  it('maps every non-off option to a positive millisecond value', () => {
    for (const option of UPDATE_INTERVAL_OPTIONS) {
      if (option.value === 'off') {
        expect(UPDATE_INTERVAL_MS).not.toHaveProperty('off')
        continue
      }
      expect(UPDATE_INTERVAL_MS[option.value]).toBeGreaterThan(0)
    }
  })

  it('rejects unknown interval keys', () => {
    expect(isUpdateInterval('weekly')).toBe(false)
    expect(isUpdateInterval('7d')).toBe(true)
  })
})
