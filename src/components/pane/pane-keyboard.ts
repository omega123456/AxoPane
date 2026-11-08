import type { KeyHandlerTable } from '@/lib/key-table'
import { arrowGridMovements, type GridMovement } from '@/lib/pane-grid'

export type KeyLike = {
  key: string
  ctrlKey: boolean
  metaKey: boolean
  altKey: boolean
}

export type RowNavigation = {
  focusedRowIndex: number
  rowCount: number
  visibleRowCount: () => number
  focusRow: (rowIndex: number) => void
}

const SINGLE_CHARACTER = 1
const SPACE_KEY = ' '
const FIRST_ROW_INDEX = 0

export function isTypeaheadKey(event: KeyLike) {
  const printable = event.key.length === SINGLE_CHARACTER && event.key !== SPACE_KEY
  const unmodified = !event.ctrlKey && !event.metaKey && !event.altKey
  return printable && unmodified
}

export function rowNavigationKeyTable(navigation: RowNavigation): KeyHandlerTable {
  const { focusedRowIndex, focusRow } = navigation
  return {
    ArrowDown: () => focusRow(focusedRowIndex + 1),
    ArrowUp: () => focusRow(focusedRowIndex - 1),
    PageDown: () => focusRow(focusedRowIndex + navigation.visibleRowCount()),
    PageUp: () => focusRow(focusedRowIndex - navigation.visibleRowCount()),
    Home: () => focusRow(FIRST_ROW_INDEX),
    End: () => focusRow(navigation.rowCount - 1),
  }
}

export function gridMovementKeyTable(onMove: (movement: GridMovement) => void): KeyHandlerTable {
  const table: KeyHandlerTable = {}
  for (const [key, movement] of Object.entries(arrowGridMovements)) {
    table[key] = () => onMove(movement)
  }
  return table
}
