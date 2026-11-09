import type { DirectoryEntry } from '@/lib/types/ipc'
import type { MarqueeRect } from './MarqueeSelection'
import type { PaneSelection } from '@/stores/selection-store'

export type ContentPoint = { x: number; y: number }

export type MarqueeDrag = {
  start: ContentPoint
  additive: boolean
  baseSelectedIds: string[]
}

export type DetailsRowGeometry = {
  listInsetPx: number
  rowHeightPx: number
  rowCount: number
  parentOffset: number
}

export const MARQUEE_DRAG_THRESHOLD_PX = 4

export function marqueeRectBetween(start: ContentPoint, point: ContentPoint): MarqueeRect | null {
  const width = Math.abs(point.x - start.x)
  const height = Math.abs(point.y - start.y)
  const belowThreshold = width < MARQUEE_DRAG_THRESHOLD_PX && height < MARQUEE_DRAG_THRESHOLD_PX
  if (belowThreshold) {
    return null
  }
  return { left: Math.min(start.x, point.x), top: Math.min(start.y, point.y), width, height }
}

export function detailsEntryIdsInRect(
  rect: MarqueeRect,
  entries: DirectoryEntry[],
  geometry: DetailsRowGeometry,
): string[] {
  const top = rect.top - geometry.listInsetPx
  const firstRow = Math.max(0, Math.floor(top / geometry.rowHeightPx))
  const lastRow = Math.min(
    geometry.rowCount - 1,
    Math.ceil((top + rect.height) / geometry.rowHeightPx) - 1,
  )
  const entryStart = Math.max(0, firstRow - geometry.parentOffset)
  const entryEnd = lastRow - geometry.parentOffset
  const coversEntries = lastRow >= firstRow && entryEnd >= entryStart
  if (!coversEntries) {
    return []
  }
  return entries.slice(entryStart, entryEnd + 1).map((entry) => entry.id)
}

export function marqueeSelection(drag: MarqueeDrag, rangeIds: string[]): PaneSelection {
  const selectedIds = drag.additive
    ? Array.from(new Set([...drag.baseSelectedIds, ...rangeIds]))
    : rangeIds
  return {
    selectedIds,
    anchorId: selectedIds[0] ?? null,
    focusedId: rangeIds.at(-1) ?? selectedIds.at(-1) ?? null,
  }
}
