import { describe, expect, it } from 'vitest'
import { formatBytes, formatCount, formatEta, formatRate } from '@/lib/format'

describe('format helpers', () => {
  it('formats byte sizes with units', () => {
    expect(formatBytes(0)).toBe('0 B')
    expect(formatBytes(512)).toBe('512 B')
    expect(formatBytes(2048)).toBe('2.0 KB')
    expect(formatBytes(5 * 1024 * 1024)).toBe('5.0 MB')
    expect(formatBytes(3 * 1024 ** 4)).toBe('3.0 TB')
  })

  it('formats rate per second', () => {
    expect(formatRate(1024)).toBe('1.0 KB/s')
  })

  it('formats ETA across magnitudes', () => {
    expect(formatEta(5)).toBe('about 5 sec left')
    expect(formatEta(120)).toBe('about 2 min left')
    expect(formatEta(7200)).toBe('about 2 hr left')
  })

  it('formats counts with separators', () => {
    expect(formatCount(1248)).toBe('1,248')
  })
})
