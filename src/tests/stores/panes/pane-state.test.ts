import {
  createPane,
  defaultPanesData,
  findMatchingEntryId,
  isHiddenFromView,
  isListingSettled,
  PANE_TITLES,
} from '@/stores/panes/pane-state'
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

describe('isHiddenFromView', () => {
  const visible = { isHidden: false, isSystem: false }
  const hidden = { isHidden: true, isSystem: false }
  const system = { isHidden: false, isSystem: true }

  it('hides hidden and system entries while hidden files are off', () => {
    expect(isHiddenFromView(hidden, false)).toBe(true)
    expect(isHiddenFromView(system, false)).toBe(true)
    expect(isHiddenFromView(visible, false)).toBe(false)
  })

  it('shows every entry while hidden files are on', () => {
    expect(isHiddenFromView(hidden, true)).toBe(false)
    expect(isHiddenFromView(system, true)).toBe(false)
  })
})

describe('createPane', () => {
  it('starts at the default session path with an unseeded history', () => {
    const pane = createPane('left', PANE_TITLES.left)
    expect(pane.path).toBe('.')
    expect(pane.historyIndex).toBe(-1)
    expect(defaultPanesData().panes.right.title).toBe('Right pane')
  })
})
