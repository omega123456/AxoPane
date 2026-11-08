import { describe, expect, it } from 'vitest'
import { selectionForClick } from '@/components/pane/click-selection'
import { fileEntry } from '@/tests/stores/panes/pane-fixtures'

const ENTRIES = ['/a', '/b', '/c', '/d'].map((path) => fileEntry(path))
const NO_MODIFIERS = { shiftKey: false, ctrlKey: false, metaKey: false }

describe('selectionForClick', () => {
  it('selects only the clicked entry without modifiers', () => {
    const selection = { selectedIds: ['/a', '/b'], anchorId: '/a' }
    expect(selectionForClick(ENTRIES, selection, '/c', NO_MODIFIERS)).toEqual({
      selectedIds: ['/c'],
      anchorId: '/c',
      focusedId: '/c',
    })
  })

  it('selects the range between the anchor and the clicked entry with shift', () => {
    const selection = { selectedIds: ['/d'], anchorId: '/d' }
    const modifiers = { ...NO_MODIFIERS, shiftKey: true }
    expect(selectionForClick(ENTRIES, selection, '/b', modifiers)).toEqual({
      selectedIds: ['/b', '/c', '/d'],
      anchorId: '/d',
      focusedId: '/b',
    })
  })

  it('falls back to a single selection when shift has no anchor', () => {
    const selection = { selectedIds: [], anchorId: null }
    const modifiers = { ...NO_MODIFIERS, shiftKey: true }
    expect(selectionForClick(ENTRIES, selection, '/b', modifiers).selectedIds).toEqual(['/b'])
  })

  it('toggles the clicked entry with ctrl or meta, keeping the anchor', () => {
    const selection = { selectedIds: ['/a', '/b'], anchorId: '/a' }
    const withCtrl = selectionForClick(ENTRIES, selection, '/b', { ...NO_MODIFIERS, ctrlKey: true })
    expect(withCtrl).toEqual({ selectedIds: ['/a'], anchorId: '/a', focusedId: '/b' })
    const withMeta = selectionForClick(ENTRIES, selection, '/c', { ...NO_MODIFIERS, metaKey: true })
    expect(withMeta).toEqual({ selectedIds: ['/a', '/b', '/c'], anchorId: '/a', focusedId: '/c' })
  })

  it('anchors a toggle on the clicked entry when nothing was anchored', () => {
    const selection = { selectedIds: [], anchorId: null }
    const modifiers = { ...NO_MODIFIERS, ctrlKey: true }
    expect(selectionForClick(ENTRIES, selection, '/c', modifiers).anchorId).toBe('/c')
  })
})
