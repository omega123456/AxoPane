type PositionedRow = { index: number; start: number }

const MINIMUM_VISIBLE_ROWS = 1

export function visibleRowCount(scrollElement: HTMLElement | null, rowPitch: number) {
  const viewportHeight = scrollElement?.clientHeight ?? 0
  return Math.max(MINIMUM_VISIBLE_ROWS, Math.floor(viewportHeight / rowPitch))
}

export type FallbackVirtualRow = { key: string; index: number; start: number }

export function virtualRowsOrFallback<Row extends PositionedRow>({
  measuredRows,
  rowCount,
  fallbackRowLimit,
  rowStart,
  keyPrefix,
}: {
  measuredRows: Row[]
  rowCount: number
  fallbackRowLimit: number
  rowStart: (index: number) => number
  keyPrefix: string
}): (Row | FallbackVirtualRow)[] {
  if (measuredRows.length > 0) {
    return measuredRows
  }
  if (rowCount === 0) {
    return measuredRows
  }
  return Array.from({ length: Math.min(rowCount, fallbackRowLimit) }, (_, index) => ({
    key: `${keyPrefix}-${index}`,
    index,
    start: rowStart(index),
  }))
}

export function virtualListHeight({
  measuredHeight,
  rowCount,
  estimated,
  trailing = 0,
}: {
  measuredHeight: number
  rowCount: number
  estimated: number
  trailing?: number
}): number {
  if (rowCount === 0) {
    return 0
  }
  const contentHeight = measuredHeight || estimated
  return contentHeight + trailing
}
