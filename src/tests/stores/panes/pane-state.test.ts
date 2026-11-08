import { findMatchingEntryId, isListingSettled } from '@/stores/panes/pane-state'
import { fileEntry, paneWith } from './pane-fixtures'

describe('isListingSettled', () => {
  it('is true once a complete listing has finished loading', () => {
    expect(isListingSettled(paneWith('left', { loading: false, listingComplete: true }))).toBe(true)
  })

  it('is false while loading or before the listing completes', () => {
    expect(isListingSettled(paneWith('left', { loading: true, listingComplete: true }))).toBe(false)
    expect(isListingSettled(paneWith('left', { loading: false, listingComplete: false }))).toBe(
      false,
    )
  })
})

describe('findMatchingEntryId', () => {
  it('returns the first candidate present in the entries, skipping empty ones', () => {
    const entries = [fileEntry('a.txt'), fileEntry('b.txt')]
    const [first, second] = entries
    expect(findMatchingEntryId(entries, [null, 'missing', second.id, first.id])).toBe(second.id)
    expect(findMatchingEntryId(entries, [undefined, 'missing'])).toBeNull()
  })
})
