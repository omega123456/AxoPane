import {
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
  type ForwardedRef,
  type RefObject,
} from 'react'
import {
  moveGridIndex,
  PANE_GRID_GAP,
  paneGridLayout,
  visualRowCount,
  type GridMovement,
} from '@/lib/pane-grid'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import type { PaneViewMode } from '@/lib/pane-view'
import { BOTH_SIDES } from '@/lib/numbers'
import type { VisibleRange } from '@/types/pane'
import { virtualListHeight, virtualRowsOrFallback } from '@/lib/virtual-rows'
import type { GraphicalThumbnailRange } from '@/lib/thumbnail-candidates'
import type { DirectoryEntry } from '@/lib/types/ipc'

const THUMBNAIL_OVERSCAN_ROWS = 2
const ICON_OVERSCAN_ROWS = 4
const UNMEASURED_FALLBACK_ROWS = 8
const FALLBACK_ROW_KEY_PREFIX = 'visual-row'
const FALLBACK_VIEWPORT_HEIGHT_PX = 480
const THUMBNAIL_PREFETCH_ROWS = 2

export type GraphicalMarqueeRect = { left: number; top: number; width: number; height: number }

export type GraphicalViewHandle = {
  scrollToEntry: (index: number) => void
  moveFocus: (movement: GridMovement) => void
  getVisibleRowCount: () => number
  getEntriesIntersectingRect: (rect: GraphicalMarqueeRect) => string[]
}

type GridLayout = ReturnType<typeof paneGridLayout>
type GridMetrics = Pick<GridLayout, 'columns' | 'rowPitch'>
type GraphicalMode = Exclude<PaneViewMode, 'details'>
type ScrollRef = RefObject<HTMLDivElement | null>

export function scrollDirection(
  previousTop: number,
  nextTop: number,
): GraphicalThumbnailRange['direction'] {
  if (nextTop > previousTop) {
    return 'down'
  }
  return nextTop < previousTop ? 'up' : 'stationary'
}

type GraphicalRowsOptions = {
  mode: GraphicalMode
  entryCount: number
  scrollRef: ScrollRef
  width: number
}

export function useGraphicalRows({ mode, entryCount, scrollRef, width }: GraphicalRowsOptions) {
  const grid = paneGridLayout(mode, width)
  const rowCount = visualRowCount(entryCount, grid.columns)
  const rowVirtualizer = useElementVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => grid.rowPitch,
    overscan: mode === 'thumbnails' ? THUMBNAIL_OVERSCAN_ROWS : ICON_OVERSCAN_ROWS,
    measurementKey: grid.rowPitch,
  })
  const rowsToRender = virtualRowsOrFallback({
    measuredRows: rowVirtualizer.getVirtualItems(),
    rowCount,
    fallbackRowLimit: UNMEASURED_FALLBACK_ROWS,
    rowStart: (index) => index * grid.rowPitch,
    keyPrefix: FALLBACK_ROW_KEY_PREFIX,
  })
  const totalHeight = virtualListHeight({
    measuredHeight: rowVirtualizer.getTotalSize(),
    rowCount,
    estimated: rowCount * grid.rowPitch,
    trailing: PANE_GRID_GAP,
  })
  return { grid, rowCount, rowVirtualizer, rowsToRender, totalHeight }
}

function visibleGridRows(scrollRef: ScrollRef, rowPitch: number) {
  return Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 0) / rowPitch))
}

type GraphicalFocusOptions = {
  entries: DirectoryEntry[]
  focusedEntryId: string | null
  grid: GridLayout
  scrollRef: ScrollRef
  onFocusEntry: (entryId: string) => void
  onKeyboardMove?: (entryId: string, movement: GridMovement) => void
}

export function useGraphicalFocus(options: GraphicalFocusOptions) {
  const { entries, focusedEntryId, grid, scrollRef, onFocusEntry, onKeyboardMove } = options
  const focusedIndex = useMemo(
    () => entries.findIndex((entry) => entry.id === focusedEntryId),
    [entries, focusedEntryId],
  )

  const moveFocus = useCallback(
    (movement: GridMovement) => {
      const next = moveGridIndex({
        index: focusedIndex < 0 ? 0 : focusedIndex,
        entryCount: entries.length,
        columns: grid.columns,
        visibleRows: visibleGridRows(scrollRef, grid.rowPitch),
        movement,
      })
      if (next === null) return
      const entry = entries[next]
      onFocusEntry(entry.id)
      onKeyboardMove?.(entry.id, movement)
    },
    [focusedIndex, grid.columns, grid.rowPitch, onFocusEntry, onKeyboardMove, entries, scrollRef],
  )

  return { focusedIndex, moveFocus }
}

type IntersectionGeometry = {
  entries: DirectoryEntry[]
  rect: GraphicalMarqueeRect
  gridWidth: number
  grid: GridMetrics
}

export function entriesIntersectingRect({ entries, rect, gridWidth, grid }: IntersectionGeometry) {
  const gridPaddingPx = PANE_GRID_GAP
  const cardWidth =
    (gridWidth - gridPaddingPx * BOTH_SIDES - PANE_GRID_GAP * (grid.columns - 1)) / grid.columns
  const cardHeight = grid.rowPitch - gridPaddingPx
  const rectRight = rect.left + rect.width
  const rectBottom = rect.top + rect.height

  if (cardWidth <= 0) {
    return []
  }
  if (cardHeight <= 0) {
    return []
  }

  return entries.flatMap((entry, index) => {
    const row = Math.floor(index / grid.columns)
    const column = index % grid.columns
    const left = gridPaddingPx + column * (cardWidth + PANE_GRID_GAP)
    const top = row * grid.rowPitch + gridPaddingPx
    const intersects =
      rect.left < left + cardWidth &&
      rectRight > left &&
      rect.top < top + cardHeight &&
      rectBottom > top
    return intersects ? [entry.id] : []
  })
}

type GraphicalHandleSources = {
  entries: DirectoryEntry[]
  grid: GridLayout
  width: number
  scrollRef: ScrollRef
  scrollToIndex: (index: number) => void
  moveFocus: (movement: GridMovement) => void
}

export function useGraphicalHandle(
  ref: ForwardedRef<GraphicalViewHandle>,
  sources: GraphicalHandleSources,
) {
  const { entries, grid, width, scrollRef, scrollToIndex, moveFocus } = sources
  useImperativeHandle(
    ref,
    () => ({
      scrollToEntry: (index) => scrollToIndex(Math.floor(index / grid.columns)),
      moveFocus,
      getVisibleRowCount: () => visibleGridRows(scrollRef, grid.rowPitch),
      getEntriesIntersectingRect: (rect) =>
        entriesIntersectingRect({
          entries,
          rect,
          gridWidth: scrollRef.current?.clientWidth || width,
          grid: { columns: grid.columns, rowPitch: grid.rowPitch },
        }),
    }),
    [grid.columns, grid.rowPitch, moveFocus, entries, scrollToIndex, scrollRef, width],
  )
}

type VirtualRowIndex = { index: number }

export type VisibleGridRows = {
  rowsToRender: VirtualRowIndex[]
  columns: number
  entryCount: number
}

const NOTHING_VISIBLE: VisibleRange = { startIndex: 0, endIndex: 0, viewportCount: 0 }

export function visibleGridRange({ rowsToRender, columns, entryCount }: VisibleGridRows) {
  const nothingVisible = rowsToRender.length === 0 || entryCount === 0
  if (nothingVisible) {
    return NOTHING_VISIBLE
  }
  const firstRow = rowsToRender[0].index
  const finalRow = rowsToRender[rowsToRender.length - 1].index
  const startIndex = firstRow * columns
  const endIndex = Math.min(entryCount - 1, (finalRow + 1) * columns - 1)
  const viewportCount = Math.max(1, finalRow - firstRow + 1) * columns
  return { startIndex, endIndex, viewportCount }
}

export function useVisibleEntryReport(
  visibleRows: VisibleGridRows,
  onVisibleRangeChange: (visibleRange: VisibleRange) => void,
) {
  const { rowsToRender, columns, entryCount } = visibleRows
  useEffect(() => {
    onVisibleRangeChange(visibleGridRange({ rowsToRender, columns, entryCount }))
  }, [columns, onVisibleRangeChange, entryCount, rowsToRender])
}

type ThumbnailRangeInput = {
  element: HTMLDivElement | null
  grid: GridMetrics
  rowCount: number
  entryCount: number
  direction: GraphicalThumbnailRange['direction']
}

function thumbnailRange(input: ThumbnailRangeInput): GraphicalThumbnailRange {
  const { element, grid, rowCount, entryCount, direction } = input
  const viewportHeight =
    element?.clientHeight || element?.getBoundingClientRect().height || FALLBACK_VIEWPORT_HEIGHT_PX
  const scrollTop = element?.scrollTop ?? 0
  const firstRow = Math.max(0, Math.floor(scrollTop / grid.rowPitch))
  const lastRow = Math.min(
    rowCount - 1,
    Math.max(firstRow, Math.ceil((scrollTop + viewportHeight) / grid.rowPitch) - 1),
  )
  const prefetchFirstRow = Math.max(0, firstRow - THUMBNAIL_PREFETCH_ROWS)
  const prefetchLastRow = Math.min(rowCount - 1, lastRow + THUMBNAIL_PREFETCH_ROWS)
  const lastEntryOfRow = (row: number) => Math.min(entryCount - 1, (row + 1) * grid.columns - 1)
  return {
    visibleStart: firstRow * grid.columns,
    visibleEnd: lastEntryOfRow(lastRow),
    prefetchStart: prefetchFirstRow * grid.columns,
    prefetchEnd: lastEntryOfRow(prefetchLastRow),
    direction,
  }
}

type ThumbnailRangeReport = {
  mode: GraphicalMode
  grid: GridLayout
  rowCount: number
  entryCount: number
  rowsToRender: VirtualRowIndex[]
  scrollRef: ScrollRef
  onThumbnailRangeChange?: (range: GraphicalThumbnailRange) => void
}

export function useScrollDirectionTracking() {
  const lastScrollTopRef = useRef(0)
  const scrollDirectionRef = useRef<GraphicalThumbnailRange['direction']>('stationary')
  const trackScrollTop = useCallback((next: number) => {
    scrollDirectionRef.current = scrollDirection(lastScrollTopRef.current, next)
    lastScrollTopRef.current = next
  }, [])
  return { scrollDirectionRef, trackScrollTop }
}

export function useThumbnailRangeReport(
  report: ThumbnailRangeReport,
  scrollDirectionRef: RefObject<GraphicalThumbnailRange['direction']>,
) {
  const { mode, grid, rowCount, entryCount, rowsToRender, scrollRef, onThumbnailRangeChange } =
    report
  useEffect(() => {
    if (mode !== 'thumbnails') return
    if (!onThumbnailRangeChange) return
    if (entryCount === 0) return
    onThumbnailRangeChange(
      thumbnailRange({
        element: scrollRef.current,
        grid: { columns: grid.columns, rowPitch: grid.rowPitch },
        rowCount,
        entryCount,
        direction: scrollDirectionRef.current,
      }),
    )
  }, [
    grid.columns,
    grid.rowPitch,
    mode,
    onThumbnailRangeChange,
    entryCount,
    rowCount,
    rowsToRender,
    scrollRef,
    scrollDirectionRef,
  ])
}

export function useScrollElementBinding(scrollContainerRef?: RefObject<HTMLDivElement | null>) {
  const internalScrollRef = useRef<HTMLDivElement | null>(null)
  const [measuredElement, setMeasuredElement] = useState<HTMLDivElement | null>(null)
  const setScrollElement = useCallback(
    (element: HTMLDivElement | null) => {
      internalScrollRef.current = element
      if (scrollContainerRef) scrollContainerRef.current = element
      setMeasuredElement(element)
    },
    [scrollContainerRef],
  )
  const scrollRef = scrollContainerRef ?? internalScrollRef
  return { scrollRef, measuredElement, setScrollElement }
}

export function useScrollFocusedRowIntoView(
  focusedIndex: number,
  columns: number,
  scrollToIndex: (index: number) => void,
) {
  useEffect(() => {
    if (focusedIndex >= 0) scrollToIndex(Math.floor(focusedIndex / columns))
  }, [focusedIndex, columns, scrollToIndex])
}
