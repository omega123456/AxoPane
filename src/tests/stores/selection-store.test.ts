import { describe, expect, it } from 'vitest'
import { EMPTY_SELECTION, singleEntrySelection, useSelectionStore } from '@/stores/selection-store'

describe('selection-store', () => {
  it('describes an empty selection with no anchor or focus', () => {
    expect(EMPTY_SELECTION).toEqual({ anchorId: null, focusedId: null, selectedIds: [] })
  })

  it('selects, anchors and focuses one entry', () => {
    expect(singleEntrySelection('Alpha')).toEqual({
      anchorId: 'Alpha',
      focusedId: 'Alpha',
      selectedIds: ['Alpha'],
    })
  })

  it('stores a whole selection per pane and clears it back to empty', () => {
    const selection = { anchorId: 'Alpha', focusedId: 'Beta', selectedIds: ['Alpha', 'Beta'] }
    useSelectionStore.getState().setSelection('right', selection)
    expect(useSelectionStore.getState().selections.right).toEqual(selection)
    expect(useSelectionStore.getState().selections.left).toEqual(EMPTY_SELECTION)

    useSelectionStore.getState().clearSelectionForPane('right')
    expect(useSelectionStore.getState().selections.right).toEqual(EMPTY_SELECTION)
  })
})
