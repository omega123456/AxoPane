import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useMemo,
  useRef,
  useState,
} from 'react'
import type { KeyboardEvent, ReactNode, RefObject, UIEvent } from 'react'
import { EntryCard } from './EntryCard'
import type { FileRowActions } from './FileRow'
import {
  moveGridIndex,
  PANE_GRID_GAP,
  paneGridLayout,
  visualRowCount,
  type GridMovement,
} from '@/lib/pane-grid'
import { pathKey } from '@/lib/path-compare'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { useElementWidth } from '@/lib/use-element-width'
import type { PaneViewMode } from '@/lib/pane-view'
import type { InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneState } from '@/types/pane'

export type GraphicalViewHandle = {
  scrollToEntry: (index: number) => void
  getVisibleRowCount: () => number
  getEntriesIntersectingRect: (rect: GraphicalMarqueeRect) => string[]
}

export type GraphicalMarqueeRect = { left: number; top: number; width: number; height: number }

export type GraphicalThumbnailRange = {
  visibleStart: number
  visibleEnd: number
  prefetchStart: number
  prefetchEnd: number
  direction: 'up' | 'down' | 'stationary'
}

type GraphicalViewProps = {
  pane: PaneState
  mode: Exclude<PaneViewMode, 'details'>
  isActivePane: boolean
  selectedIds: Set<string>
  cutEntryPaths: Set<string>
  dropTargetEntryId: string | null
  rename: InlineRenameState | null
  actions: FileRowActions
  onVisibleRangeChange: (start: number, end: number, viewportCount: number) => void
  onThumbnailRangeChange?: (range: GraphicalThumbnailRange) => void
  onFocusEntry: (entryId: string) => void
  onKeyboardMove?: (entryId: string, movement: GridMovement) => void
  onContainerMouseDown?: (event: React.MouseEvent<HTMLDivElement>) => void
  onScroll?: (position: number) => void
  onContainerContextMenu?: (event: React.MouseEvent<HTMLDivElement>) => void
  onPaneDragOver?: (event: React.DragEvent<HTMLDivElement>) => void
  onPaneDragLeave?: (event: React.DragEvent<HTMLDivElement>) => void
  onPaneDrop?: (event: React.DragEvent<HTMLDivElement>) => void
  isPaneDropTarget?: boolean
  marqueeRect?: { left: number; top: number; width: number; height: number } | null
  body?: ReactNode
  scrollContainerRef?: RefObject<HTMLDivElement | null>
}

export const GraphicalView = forwardRef<GraphicalViewHandle, GraphicalViewProps>(
  function GraphicalView(
    {
      pane,
      mode,
      isActivePane,
      selectedIds,
      cutEntryPaths,
      dropTargetEntryId,
      rename,
      actions,
      onVisibleRangeChange,
      onThumbnailRangeChange,
      onFocusEntry,
      onKeyboardMove,
      onContainerMouseDown,
      onScroll,
      onContainerContextMenu,
      onPaneDragOver,
      onPaneDragLeave,
      onPaneDrop,
      isPaneDropTarget = false,
      marqueeRect,
      body,
      scrollContainerRef,
    },
    ref,
  ) {
    const internalScrollRef = useRef<HTMLDivElement | null>(null)
    const lastScrollTopRef = useRef(0)
    const scrollDirectionRef = useRef<GraphicalThumbnailRange['direction']>('stationary')
    const [measuredElement, setMeasuredElement] = useState<HTMLDivElement | null>(null)
    const width = useElementWidth(measuredElement)
    const grid = paneGridLayout(mode, width)
    const rowCount = visualRowCount(pane.entries.length, grid.columns)
    const scrollRef = scrollContainerRef ?? internalScrollRef
    const rowVirtualizer = useElementVirtualizer({
      count: rowCount,
      getScrollElement: () => scrollRef.current,
      estimateSize: () => grid.rowPitch,
      overscan: mode === 'thumbnails' ? 2 : 4,
      measurementKey: grid.rowPitch,
    })
    const virtualRows = rowVirtualizer.getVirtualItems()
    const rowsToRender =
      virtualRows.length > 0 || rowCount === 0
        ? virtualRows
        : Array.from({ length: Math.min(rowCount, 8) }, (_, index) => ({
            key: `visual-row-${index}`,
            index,
            start: index * grid.rowPitch,
          }))
    const totalHeight =
      rowCount === 0 ? 0 : rowVirtualizer.getTotalSize() || rowCount * grid.rowPitch

    const setScrollElement = useCallback(
      (element: HTMLDivElement | null) => {
        internalScrollRef.current = element
        if (scrollContainerRef) scrollContainerRef.current = element
        setMeasuredElement(element)
      },
      [scrollContainerRef],
    )

    const focusedIndex = useMemo(
      () => pane.entries.findIndex((entry) => entry.id === pane.focusedEntryId),
      [pane.entries, pane.focusedEntryId],
    )

    useImperativeHandle(
      ref,
      () => ({
        scrollToEntry: (index) => rowVirtualizer.scrollToIndex(Math.floor(index / grid.columns)),
        getVisibleRowCount: () =>
          Math.max(1, Math.floor((scrollRef.current?.clientHeight ?? 0) / grid.rowPitch)),
        getEntriesIntersectingRect: (rect) => {
          // Grid rows have `p-1` (4px on every side) and `gap-gap`. The remaining
          // width is divided evenly by CSS Grid, so this gives every card's real
          // rectangle without relying on which virtual rows happen to be mounted.
          const gridWidth = scrollRef.current?.clientWidth || width
          const gridPaddingPx = 4
          const cardWidth =
            (gridWidth - gridPaddingPx * 2 - PANE_GRID_GAP * (grid.columns - 1)) / grid.columns
          const cardHeight = grid.rowPitch - gridPaddingPx * 2
          const rectRight = rect.left + rect.width
          const rectBottom = rect.top + rect.height

          if (cardWidth <= 0 || cardHeight <= 0) {
            return []
          }

          return pane.entries.flatMap((entry, index) => {
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
        },
      }),
      [grid.columns, grid.rowPitch, pane.entries, rowVirtualizer, scrollRef, width],
    )

    useEffect(() => {
      if (focusedIndex >= 0) rowVirtualizer.scrollToIndex(Math.floor(focusedIndex / grid.columns))
    }, [focusedIndex, grid.columns, rowVirtualizer])

    useEffect(() => {
      if (rowsToRender.length === 0 || pane.entries.length === 0) {
        onVisibleRangeChange(0, 0, 0)
        return
      }
      const first = rowsToRender[0].index * grid.columns
      const finalRow = rowsToRender[rowsToRender.length - 1].index
      const last = Math.min(pane.entries.length - 1, (finalRow + 1) * grid.columns - 1)
      onVisibleRangeChange(
        first,
        last,
        Math.max(1, finalRow - rowsToRender[0].index + 1) * grid.columns,
      )
    }, [grid.columns, onVisibleRangeChange, pane.entries.length, rowsToRender])

    useEffect(() => {
      if (mode !== 'thumbnails' || !onThumbnailRangeChange || pane.entries.length === 0) return
      const element = scrollRef.current
      const viewportHeight = element?.clientHeight || element?.getBoundingClientRect().height || 480
      const scrollTop = element?.scrollTop ?? 0
      const firstRow = Math.max(0, Math.floor(scrollTop / grid.rowPitch))
      const lastRow = Math.min(
        rowCount - 1,
        Math.max(firstRow, Math.ceil((scrollTop + viewportHeight) / grid.rowPitch) - 1),
      )
      const prefetchFirstRow = Math.max(0, firstRow - 2)
      const prefetchLastRow = Math.min(rowCount - 1, lastRow + 2)
      onThumbnailRangeChange({
        visibleStart: firstRow * grid.columns,
        visibleEnd: Math.min(pane.entries.length - 1, (lastRow + 1) * grid.columns - 1),
        prefetchStart: prefetchFirstRow * grid.columns,
        prefetchEnd: Math.min(pane.entries.length - 1, (prefetchLastRow + 1) * grid.columns - 1),
        direction: scrollDirectionRef.current,
      })
    }, [
      grid.columns,
      grid.rowPitch,
      mode,
      onThumbnailRangeChange,
      pane.entries.length,
      rowCount,
      rowsToRender,
      scrollRef,
    ])

    function keyboardMovement(event: KeyboardEvent<HTMLDivElement>): GridMovement | null {
      if (event.ctrlKey || event.metaKey) {
        if (event.key === 'Home') return 'first'
        if (event.key === 'End') return 'last'
      }
      if (event.key === 'ArrowLeft') return 'left'
      if (event.key === 'ArrowRight') return 'right'
      if (event.key === 'ArrowUp') return 'up'
      if (event.key === 'ArrowDown') return 'down'
      if (event.key === 'Home') return 'home'
      if (event.key === 'End') return 'end'
      if (event.key === 'PageUp') return 'pageUp'
      if (event.key === 'PageDown') return 'pageDown'
      return null
    }

    function onGridKeyDown(event: KeyboardEvent<HTMLDivElement>) {
      const movement = keyboardMovement(event)
      if (!movement || pane.entries.length === 0) return
      event.preventDefault()
      const next = moveGridIndex({
        index: focusedIndex < 0 ? 0 : focusedIndex,
        entryCount: pane.entries.length,
        columns: grid.columns,
        visibleRows: Math.max(
          1,
          Math.floor((scrollRef.current?.clientHeight ?? 0) / grid.rowPitch),
        ),
        movement,
      })
      if (next === null) return
      const entry = pane.entries[next]
      onFocusEntry(entry.id)
      onKeyboardMove?.(entry.id, movement)
    }

    if (body)
      return (
        <div
          ref={setScrollElement}
          className={`relative min-h-0 flex-1 ${isPaneDropTarget ? 'outline outline-2 -outline-offset-2 outline-accent-blue-border' : ''}`}
          onMouseDown={onContainerMouseDown}
          onContextMenu={onContainerContextMenu}
          onDragOver={onPaneDragOver}
          onDragLeave={onPaneDragLeave}
          onDrop={onPaneDrop}
        >
          {body}
        </div>
      )

    return (
      <div className="relative min-h-0 flex-1">
        <div
          ref={setScrollElement}
          data-testid={`graphical-view-scroll-${pane.id}`}
          role="grid"
          aria-label={`${mode === 'icons' ? 'Icons' : 'Large thumbnails'} for ${pane.path}`}
          aria-rowcount={rowCount}
          aria-colcount={grid.columns}
          tabIndex={0}
          className={`h-full min-h-0 overflow-auto overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border dark:scrollbar-thumb-dark-text-faint ${isPaneDropTarget ? 'outline outline-2 -outline-offset-2 outline-accent-blue-border' : ''}`}
          onKeyDown={onGridKeyDown}
          onMouseDown={onContainerMouseDown}
          onScroll={(event: UIEvent<HTMLDivElement>) => {
            const next = event.currentTarget.scrollTop
            scrollDirectionRef.current =
              next > lastScrollTopRef.current
                ? 'down'
                : next < lastScrollTopRef.current
                  ? 'up'
                  : 'stationary'
            lastScrollTopRef.current = next
            onScroll?.(next)
          }}
          onContextMenu={onContainerContextMenu}
          onDragOver={onPaneDragOver}
          onDragLeave={onPaneDragLeave}
          onDrop={onPaneDrop}
        >
          <div className="relative min-h-full w-full" style={{ height: `${totalHeight}px` }}>
            {rowsToRender.map((virtualRow) => {
              const start = virtualRow.index * grid.columns
              const entries = pane.entries.slice(start, start + grid.columns)
              return (
                <div
                  key={virtualRow.key}
                  role="row"
                  aria-rowindex={virtualRow.index + 1}
                  className={`absolute inset-x-0 grid gap-gap p-1 ${grid.className}`}
                  style={{ top: `${virtualRow.start}px`, height: `${grid.rowPitch}px` }}
                >
                  {entries.map((entry, column) => (
                    <EntryCard
                      key={entry.id}
                      entry={entry}
                      mode={mode}
                      isActivePane={isActivePane}
                      isFocused={pane.focusedEntryId === entry.id}
                      isSelected={selectedIds.has(entry.id)}
                      isCut={cutEntryPaths.has(pathKey(entry.path))}
                      isDropTarget={dropTargetEntryId === entry.id}
                      isRenaming={rename?.entryId === entry.id}
                      renameValue={rename?.entryId === entry.id ? rename.value : ''}
                      renameBusy={rename?.entryId === entry.id ? rename.busy : false}
                      renameError={rename?.entryId === entry.id ? rename.error : null}
                      draggable={!entry.trashId}
                      rowIndex={virtualRow.index + 1}
                      columnIndex={column + 1}
                      actions={actions}
                    />
                  ))}
                </div>
              )
            })}
            {marqueeRect ? (
              <div
                data-testid="marquee-selection"
                className="pointer-events-none absolute border border-accent-blue-border bg-accent-blue-soft"
                style={{
                  left: `${marqueeRect.left}px`,
                  top: `${marqueeRect.top}px`,
                  width: `${marqueeRect.width}px`,
                  height: `${marqueeRect.height}px`,
                }}
              />
            ) : null}
          </div>
        </div>
      </div>
    )
  },
)
