import { describe, expect, it } from 'vitest'
import { compareNaturally } from '@/lib/natural-order'

describe('compareNaturally', () => {
  it('orders embedded numbers by value', () => {
    expect(['file10', 'file2', 'file1'].sort(compareNaturally)).toEqual([
      'file1',
      'file2',
      'file10',
    ])
  })

  it('ignores case and accents', () => {
    expect(compareNaturally('alpha', 'ALPHA')).toBe(0)
    expect(compareNaturally('resume', 'résumé')).toBe(0)
  })
})
