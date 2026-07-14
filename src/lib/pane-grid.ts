import type { PaneViewMode } from './pane-view'

export const PANE_GRID_GAP = 8
export const PANE_GRID_MAX_COLUMNS = 12

export const paneGridClassNames: Record<number, string> = {
  1: 'grid-cols-1',
  2: 'grid-cols-2',
  3: 'grid-cols-3',
  4: 'grid-cols-4',
  5: 'grid-cols-5',
  6: 'grid-cols-6',
  7: 'grid-cols-7',
  8: 'grid-cols-8',
  9: 'grid-cols-9',
  10: 'grid-cols-10',
  11: 'grid-cols-11',
  12: 'grid-cols-12',
}

export type PaneGridLayout = {
  columns: number
  cellMinimum: number
  rowPitch: number
  className: string
}

export function paneGridMinimumCell(mode: Exclude<PaneViewMode, 'details'>) {
  return mode === 'icons' ? 208 : 208
}

export function paneGridRowPitch(mode: Exclude<PaneViewMode, 'details'>) {
  return mode === 'icons' ? 64 : 228
}

export function columnsForPaneWidth(width: number, cellMinimum: number) {
  const safeWidth = Math.max(0, width)
  const fittingColumns = Math.floor((safeWidth + PANE_GRID_GAP) / (cellMinimum + PANE_GRID_GAP))
  return Math.min(PANE_GRID_MAX_COLUMNS, Math.max(1, fittingColumns))
}

export function paneGridLayout(
  mode: Exclude<PaneViewMode, 'details'>,
  width: number,
): PaneGridLayout {
  const cellMinimum = paneGridMinimumCell(mode)
  const columns = columnsForPaneWidth(width, cellMinimum)
  return {
    columns,
    cellMinimum,
    rowPitch: paneGridRowPitch(mode),
    className: paneGridClassNames[columns],
  }
}

export function visualRowCount(entryCount: number, columns: number) {
  return Math.ceil(Math.max(0, entryCount) / Math.max(1, columns))
}

export function visualRowForIndex(index: number, columns: number) {
  return Math.floor(Math.max(0, index) / Math.max(1, columns))
}

export function visualColumnForIndex(index: number, columns: number) {
  return Math.max(0, index) % Math.max(1, columns)
}

export function entryIndexForPosition(
  row: number,
  column: number,
  entryCount: number,
  columns: number,
) {
  const index = Math.max(0, row) * Math.max(1, columns) + Math.max(0, column)
  return index < entryCount ? index : null
}

export type GridMovement =
  | 'left'
  | 'right'
  | 'up'
  | 'down'
  | 'home'
  | 'end'
  | 'first'
  | 'last'
  | 'pageUp'
  | 'pageDown'

export function moveGridIndex({
  index,
  entryCount,
  columns,
  visibleRows,
  movement,
}: {
  index: number
  entryCount: number
  columns: number
  visibleRows: number
  movement: GridMovement
}) {
  if (entryCount === 0) return null
  const lastIndex = entryCount - 1
  const current = Math.min(Math.max(0, index), lastIndex)
  const safeColumns = Math.max(1, columns)
  const currentRow = visualRowForIndex(current, safeColumns)
  const currentColumn = visualColumnForIndex(current, safeColumns)
  const lastRow = visualRowForIndex(lastIndex, safeColumns)
  const rowStart = currentRow * safeColumns
  const rowEnd = Math.min(lastIndex, rowStart + safeColumns - 1)
  const rowsPerPage = Math.max(1, visibleRows)

  switch (movement) {
    case 'left':
      return Math.max(0, current - 1)
    case 'right':
      return Math.min(lastIndex, current + 1)
    case 'up':
      return Math.max(0, current - safeColumns)
    case 'down':
      return Math.min(lastIndex, current + safeColumns)
    case 'home':
      return rowStart
    case 'end':
      return rowEnd
    case 'first':
      return 0
    case 'last':
      return lastIndex
    case 'pageUp': {
      const targetRow = Math.max(0, currentRow - rowsPerPage)
      return Math.min(lastIndex, targetRow * safeColumns + currentColumn)
    }
    case 'pageDown': {
      const targetRow = Math.min(lastRow, currentRow + rowsPerPage)
      return Math.min(lastIndex, targetRow * safeColumns + currentColumn)
    }
  }
}
