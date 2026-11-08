import { describe, expect, it, vi } from 'vitest'
import {
  gridMovementKeyTable,
  isTypeaheadKey,
  rowNavigationKeyTable,
} from '@/components/pane/pane-keyboard'

const PLAIN = { ctrlKey: false, metaKey: false, altKey: false }

describe('pane keyboard', () => {
  it('treats unmodified printable characters other than space as typeahead', () => {
    expect(isTypeaheadKey({ key: 'a', ...PLAIN })).toBe(true)
    expect(isTypeaheadKey({ key: ' ', ...PLAIN })).toBe(false)
    expect(isTypeaheadKey({ key: 'Enter', ...PLAIN })).toBe(false)
    expect(isTypeaheadKey({ key: 'a', ...PLAIN, metaKey: true })).toBe(false)
  })

  it('moves focus by row, by page and to either end', () => {
    const focusRow = vi.fn()
    const table = rowNavigationKeyTable({
      focusedRowIndex: 5,
      rowCount: 20,
      visibleRowCount: () => 4,
      focusRow,
    })
    for (const key of ['ArrowDown', 'ArrowUp', 'PageDown', 'PageUp', 'Home', 'End']) {
      table[key]?.()
    }
    expect(focusRow.mock.calls.map(([rowIndex]) => rowIndex)).toEqual([6, 4, 9, 1, 0, 19])
  })

  it('maps every arrow key to a grid movement', () => {
    const onMove = vi.fn()
    const table = gridMovementKeyTable(onMove)
    table.ArrowRight?.()
    expect(onMove).toHaveBeenCalledTimes(1)
    expect(Object.keys(table)).toEqual(
      expect.arrayContaining(['ArrowLeft', 'ArrowRight', 'ArrowUp', 'ArrowDown']),
    )
  })
})
