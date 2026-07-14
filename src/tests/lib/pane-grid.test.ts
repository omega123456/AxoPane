import {
  columnsForPaneWidth,
  entryIndexForPosition,
  moveGridIndex,
  paneGridClassNames,
  paneGridLayout,
  PANE_GRID_GAP,
  visualColumnForIndex,
  visualRowCount,
  visualRowForIndex,
} from '@/lib/pane-grid'

describe('pane grid geometry', () => {
  it('uses exact complete-cell plus gap thresholds through twelve columns', () => {
    for (let columns = 1; columns <= 12; columns += 1) {
      const threshold = columns * 208 + (columns - 1) * PANE_GRID_GAP
      expect(columnsForPaneWidth(threshold - 1, 208)).toBe(columns - 1 || 1)
      expect(columnsForPaneWidth(threshold, 208)).toBe(columns)
      expect(paneGridClassNames[columns]).toBe(`grid-cols-${columns}`)
    }
  })

  it('clamps narrow and ultra-wide layouts while retaining the mode geometry', () => {
    expect(paneGridLayout('icons', 0)).toMatchObject({ columns: 1, cellMinimum: 208, rowPitch: 64 })
    expect(paneGridLayout('thumbnails', 100000)).toMatchObject({
      columns: 12,
      cellMinimum: 208,
      rowPitch: 228,
    })
  })

  it('maps entries to complete row-major visual positions', () => {
    expect(visualRowCount(25, 4)).toBe(7)
    expect(visualRowForIndex(10, 4)).toBe(2)
    expect(visualColumnForIndex(10, 4)).toBe(2)
    expect(entryIndexForPosition(2, 2, 25, 4)).toBe(10)
    expect(entryIndexForPosition(6, 1, 25, 4)).toBe(null)
  })

  it('moves through visual geometry while preserving row bounds', () => {
    const input = { index: 6, entryCount: 10, columns: 4, visibleRows: 2 }
    expect(moveGridIndex({ ...input, movement: 'left' })).toBe(5)
    expect(moveGridIndex({ ...input, movement: 'right' })).toBe(7)
    expect(moveGridIndex({ ...input, movement: 'up' })).toBe(2)
    expect(moveGridIndex({ ...input, movement: 'down' })).toBe(9)
    expect(moveGridIndex({ ...input, movement: 'home' })).toBe(4)
    expect(moveGridIndex({ ...input, movement: 'end' })).toBe(7)
    expect(moveGridIndex({ ...input, movement: 'first' })).toBe(0)
    expect(moveGridIndex({ ...input, movement: 'last' })).toBe(9)
    expect(moveGridIndex({ ...input, movement: 'pageUp' })).toBe(2)
    expect(moveGridIndex({ ...input, movement: 'pageDown' })).toBe(9)
  })
})
