import { moveItem, withKeyFlags } from '@/lib/collections'

describe('withKeyFlags', () => {
  it('adds flagged keys and removes unflagged ones without mutating the input', () => {
    const original = { kept: true as const, removed: true as const }
    expect(withKeyFlags(original, ['added'], true)).toEqual({
      kept: true,
      removed: true,
      added: true,
    })
    expect(withKeyFlags(original, ['removed'], false)).toEqual({ kept: true })
    expect(original).toEqual({ kept: true, removed: true })
  })
})

describe('moveItem', () => {
  it('moves one item to a new index in a copy', () => {
    const items = ['a', 'b', 'c', 'd']
    expect(moveItem(items, 0, 2)).toEqual(['b', 'c', 'a', 'd'])
    expect(moveItem(items, 3, 0)).toEqual(['d', 'a', 'b', 'c'])
    expect(items).toEqual(['a', 'b', 'c', 'd'])
  })
})
