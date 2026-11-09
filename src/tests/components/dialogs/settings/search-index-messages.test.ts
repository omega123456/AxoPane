import { describe, expect, it } from 'vitest'
import {
  addedPaths,
  focusAfterAdd,
  focusAfterRemove,
  messagesForOutcomes,
  removalMessages,
} from '@/components/dialogs/settings/search-index-messages'

const A = '/Users/alex/a'
const B = '/Users/alex/b'
const C = '/Users/alex/c'

describe('search-index-messages', () => {
  it('counts every adding outcome and focuses the last one', () => {
    const outcomes = [
      { path: A, outcome: 'added' as const },
      { path: B, outcome: 'notFound' as const },
      { path: C, outcome: 'mergedChildren' as const, mergedCount: 1 },
    ]

    expect(addedPaths(outcomes)).toEqual([A, C])
    expect(focusAfterAdd(outcomes)).toBe(C)
    expect(messagesForOutcomes(outcomes)).toEqual([
      { tone: 'info', text: 'Added 2 folders.' },
      { tone: 'danger', text: `${B} doesn't exist.` },
      {
        tone: 'info',
        text: 'c already contained an excluded folder, so the list now shows only c.',
      },
    ])
  })

  it('shows repeated messages once', () => {
    const outcomes = [
      { path: B, outcome: 'notFound' as const },
      { path: B, outcome: 'notFound' as const },
    ]

    expect(messagesForOutcomes(outcomes)).toEqual([{ tone: 'danger', text: `${B} doesn't exist.` }])
  })

  it('focuses a duplicate row, or nothing when nothing was added', () => {
    expect(focusAfterAdd([{ path: A, outcome: 'duplicate', detail: B }])).toBe(B)
    expect(focusAfterAdd([{ path: A, outcome: 'duplicate' }])).toBe(A)
    expect(focusAfterAdd([{ path: A, outcome: 'rootRejected' }])).toBeNull()
    expect(focusAfterAdd([])).toBeNull()
    expect(messagesForOutcomes([])).toEqual([])
  })

  it('names the covering folder, falling back to the path itself', () => {
    expect(messagesForOutcomes([{ path: A, outcome: 'insideExisting' }])).toEqual([
      {
        tone: 'info',
        text: 'Already inside a, which is excluded. Remove that exclusion to index this folder.',
      },
    ])
  })

  it('focuses the next row after a removal, then the previous, then the path field', () => {
    expect(focusAfterRemove([A, B, C], A)).toBe(B)
    expect(focusAfterRemove([A, B, C], C)).toBe(B)
    expect(focusAfterRemove([A], A)).toBeNull()
    expect(focusAfterRemove([A], B)).toBeNull()
    expect(removalMessages(B)).toEqual([
      { tone: 'info', text: 'Removed b. Updating the index in the background.' },
    ])
  })
})
