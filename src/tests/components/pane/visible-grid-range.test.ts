import { describe, expect, it } from 'vitest'
import { visibleGridRange } from '@/components/pane/use-graphical-view'

describe('visibleGridRange', () => {
  it('reports nothing visible when no rows render or the pane is empty', () => {
    const empty = { startIndex: 0, endIndex: 0, viewportCount: 0 }
    expect(visibleGridRange({ rowsToRender: [], columns: 4, entryCount: 10 })).toEqual(empty)
    expect(visibleGridRange({ rowsToRender: [{ index: 0 }], columns: 4, entryCount: 0 })).toEqual(
      empty,
    )
  })

  it('spans every entry in the rendered rows, clamped to the last entry', () => {
    const rowsToRender = [{ index: 1 }, { index: 2 }]
    expect(visibleGridRange({ rowsToRender, columns: 4, entryCount: 10 })).toEqual({
      startIndex: 4,
      endIndex: 9,
      viewportCount: 8,
    })
  })
})
