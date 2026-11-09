import { describe, expect, it } from 'vitest'
import {
  countLabel,
  EMPTY_VALUE,
  formatBytes,
  formatCount,
  formatEta,
  formatItemCount,
  formatRate,
  itemNoun,
} from '@/lib/format'

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

describe('item counts', () => {
  it('uses the singular noun only for exactly one item', () => {
    expect(itemNoun(1)).toBe('item')
    expect(itemNoun(0)).toBe('items')
    expect(itemNoun(2)).toBe('items')
  })

  it('formats a count with thousands separators and its noun', () => {
    expect(formatItemCount(1)).toBe('1 item')
    expect(formatItemCount(1248)).toBe('1,248 items')
  })

  it('labels any count with its noun, pluralised unless it is exactly one', () => {
    expect(countLabel(1, 'folder')).toBe('1 folder')
    expect(countLabel(0, 'file')).toBe('0 files')
    expect(countLabel(4193422, 'file')).toBe('4,193,422 files')
  })

  it('shows an em dash for an empty value', () => {
    expect(EMPTY_VALUE).toBe('—')
  })
})
