import { describe, expect, it } from 'vitest'
import { formatItemPreview, verb } from '@/lib/queue-format'
import type { OpKind, OpProgress } from '@/lib/types/ipc'

function operationWithKind(kind: OpKind): OpProgress {
  return { kind } as OpProgress
}

describe('verb', () => {
  it('maps each operation kind to its present-participle verb', () => {
    expect(verb(operationWithKind('copy'))).toBe('Copying')
    expect(verb(operationWithKind('move'))).toBe('Moving')
    expect(verb(operationWithKind('delete'))).toBe('Deleting')
    expect(verb(operationWithKind('compress'))).toBe('Compressing')
    expect(verb(operationWithKind('extract'))).toBe('Extracting')
  })
})

describe('formatItemPreview', () => {
  it('returns null when there are no item names', () => {
    expect(formatItemPreview([], 0)).toBeNull()
  })

  it('joins all names when total is 2 or fewer', () => {
    expect(formatItemPreview(['a.txt'], 1)).toBe('a.txt')
    expect(formatItemPreview(['a.txt', 'b.txt'], 2)).toBe('a.txt, b.txt')
  })

  it('previews the first two names and counts the rest as "+K more"', () => {
    expect(formatItemPreview(['a.txt', 'b.txt'], 5)).toBe('a.txt, b.txt, +3 more')
  })
})
