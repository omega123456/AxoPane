import { forwardRef } from 'react'
import type { ForwardedRef, KeyboardEvent, ReactNode, RefObject, UIEvent } from 'react'
import { EntryCard } from './EntryCard'
import { MarqueeSelection } from './MarqueeSelection'
import type { FileRowActions } from './FileRow'
import { gridKeyMovement, type GridMovement } from '@/lib/pane-grid'
import { pathKey } from '@/lib/path-compare'
import { useElementWidth } from '@/lib/use-element-width'
import type { PaneViewMode } from '@/lib/pane-view'
import { entryRenameProps, type InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneState } from '@/types/pane'
import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { dropTargetOutline, SCROLLBAR_THIN } from '@/lib/class-names'
import type { GraphicalThumbnailRange } from '@/lib/thumbnail-candidates'
import {
  useGraphicalFocus,
  useGraphicalHandle,
  useGraphicalRows,
  useScrollElementBinding,
  useScrollFocusedRowIntoView,
  useScrollDirectionTracking,
  useThumbnailRangeReport,
  useVisibleEntryReport,
  type GraphicalMarqueeRect,
  type GraphicalViewHandle,
} from './use-graphical-view'

export type { GraphicalMarqueeRect, GraphicalViewHandle }
export type { GraphicalThumbnailRange }

type GraphicalMode = Exclude<PaneViewMode, 'details'>

type GraphicalViewProps = {
  pane: PaneState
  mode: GraphicalMode
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
  marqueeRect?: GraphicalMarqueeRect | null
  body?: ReactNode
  scrollContainerRef?: RefObject<HTMLDivElement | null>
}

type ContainerEventProps = Pick<
  GraphicalViewProps,
  | 'onContainerMouseDown'
  | 'onContainerContextMenu'
  | 'onPaneDragOver'
  | 'onPaneDragLeave'
  | 'onPaneDrop'
  | 'isPaneDropTarget'
>

type GraphicalCardProps = Pick<
  GraphicalViewProps,
  | 'pane'
  | 'mode'
  | 'isActivePane'
  | 'selectedIds'
  | 'cutEntryPaths'
  | 'dropTargetEntryId'
  | 'rename'
  | 'actions'
>

type GraphicalRowProps = GraphicalCardProps & {
  virtualRow: { index: number; start: number }
  columns: number
  rowPitch: number
  gridClassName: string
}

type GraphicalGridProps = GraphicalCardProps &
  ContainerEventProps & {
    setScrollElement: (element: HTMLDivElement | null) => void
    rowCount: number
    columns: number
    rowPitch: number
    gridClassName: string
    totalHeight: number
    rowsToRender: Array<{ index: number; start: number; key: React.Key }>
    marqueeRect?: GraphicalMarqueeRect | null
    onGridKeyDown: (event: KeyboardEvent<HTMLDivElement>) => void
    onGridScroll: (event: UIEvent<HTMLDivElement>) => void
  }

function gridAriaLabel(mode: GraphicalMode, path: string) {
  const modeLabel = mode === 'icons' ? 'Icons' : 'Large thumbnails'
  return `${modeLabel} for ${path}`
}

function GraphicalRow({
  virtualRow,
  columns,
  rowPitch,
  gridClassName,
  ...card
}: GraphicalRowProps) {
  const start = virtualRow.index * columns
  const entries = card.pane.entries.slice(start, start + columns)
  return (
    <div
      role="row"
      aria-rowindex={virtualRow.index + 1}
      className={`absolute inset-x-0 grid gap-1 px-1 pt-1 ${gridClassName}`}
      style={{ top: `${virtualRow.start}px`, height: `${rowPitch}px` }}
    >
      {entries.map((entry, column) => (
        <EntryCard
          key={entry.id}
          entry={entry}
          mode={card.mode}
          isActivePane={card.isActivePane}
          isFocused={card.pane.focusedEntryId === entry.id}
          isSelected={card.selectedIds.has(entry.id)}
          isCut={card.cutEntryPaths.has(pathKey(entry.path))}
          isDropTarget={card.dropTargetEntryId === entry.id}
          {...entryRenameProps(card.rename, entry.id)}
          draggable={!entry.trashId}
          rowIndex={virtualRow.index + 1}
          columnIndex={column + 1}
          actions={card.actions}
        />
      ))}
    </div>
  )
}

function GraphicalBodyContainer({
  setScrollElement,
  body,
  ...events
}: ContainerEventProps & {
  setScrollElement: (element: HTMLDivElement | null) => void
  body: ReactNode
}) {
  return (
    <div
      ref={setScrollElement}
      className={`relative min-h-0 flex-1 ${dropTargetOutline(events.isPaneDropTarget ?? false)}`}
      onMouseDown={events.onContainerMouseDown}
      onContextMenu={events.onContainerContextMenu}
      onDragOver={events.onPaneDragOver}
      onDragLeave={events.onPaneDragLeave}
      onDrop={events.onPaneDrop}
    >
      {body}
    </div>
  )
}

function GraphicalGrid({ setScrollElement, ...props }: GraphicalGridProps) {
  const { pane, mode, rowsToRender, columns, rowPitch, gridClassName } = props
  return (
    <div className="relative min-h-0 flex-1">
      <div
        ref={setScrollElement}
        data-testid={`graphical-view-scroll-${pane.id}`}
        role="grid"
        aria-label={gridAriaLabel(mode, pane.path)}
        aria-rowcount={props.rowCount}
        aria-colcount={columns}
        tabIndex={0}
        className={`h-full min-h-0 overflow-auto overscroll-contain ${SCROLLBAR_THIN} ${FOCUS_RING_INSET} ${dropTargetOutline(props.isPaneDropTarget ?? false)}`}
        onKeyDown={props.onGridKeyDown}
        onMouseDown={props.onContainerMouseDown}
        onScroll={props.onGridScroll}
        onContextMenu={props.onContainerContextMenu}
        onDragOver={props.onPaneDragOver}
        onDragLeave={props.onPaneDragLeave}
        onDrop={props.onPaneDrop}
      >
        <div className="relative min-h-full w-full" style={{ height: `${props.totalHeight}px` }}>
          {rowsToRender.map((virtualRow) => (
            <GraphicalRow
              key={virtualRow.key}
              {...props}
              virtualRow={virtualRow}
              columns={columns}
              rowPitch={rowPitch}
              gridClassName={gridClassName}
            />
          ))}
          <MarqueeSelection rect={props.marqueeRect} />
        </div>
      </div>
    </div>
  )
}

function useGraphicalViewModel(props: GraphicalViewProps, ref: ForwardedRef<GraphicalViewHandle>) {
  const { pane, mode } = props
  const entryCount = pane.entries.length
  const { scrollRef, measuredElement, setScrollElement } = useScrollElementBinding(
    props.scrollContainerRef,
  )
  const width = useElementWidth(measuredElement)
  const rows = useGraphicalRows(mode, entryCount, scrollRef, width)
  const { grid, rowCount, rowsToRender } = rows
  const { focusedIndex, moveFocus } = useGraphicalFocus({
    entries: pane.entries,
    focusedEntryId: pane.focusedEntryId,
    grid,
    scrollRef,
    onFocusEntry: props.onFocusEntry,
    onKeyboardMove: props.onKeyboardMove,
  })
  const { scrollDirectionRef, trackScrollTop } = useScrollDirectionTracking()
  const scrollToIndex = rows.rowVirtualizer.scrollToIndex
  useGraphicalHandle(ref, {
    entries: pane.entries,
    grid,
    width,
    scrollRef,
    scrollToIndex,
    moveFocus,
  })
  useScrollFocusedRowIntoView(focusedIndex, grid.columns, scrollToIndex)
  useVisibleEntryReport(rowsToRender, grid.columns, entryCount, props.onVisibleRangeChange)
  useThumbnailRangeReport(
    {
      mode,
      grid,
      rowCount,
      entryCount,
      rowsToRender,
      scrollRef,
      onThumbnailRangeChange: props.onThumbnailRangeChange,
    },
    scrollDirectionRef,
  )

  function onGridKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const movement = gridKeyMovement(event)
    if (!movement) return
    if (entryCount === 0) return
    event.preventDefault()
    moveFocus(movement)
  }

  function onGridScroll(event: UIEvent<HTMLDivElement>) {
    const nextScrollTop = event.currentTarget.scrollTop
    trackScrollTop(nextScrollTop)
    props.onScroll?.(nextScrollTop)
  }

  return { ...rows, setScrollElement, onGridKeyDown, onGridScroll }
}

export const GraphicalView = forwardRef<GraphicalViewHandle, GraphicalViewProps>(
  function GraphicalView(props, ref) {
    const model = useGraphicalViewModel(props, ref)
    const { setScrollElement, grid } = model

    if (props.body)
      return (
        <GraphicalBodyContainer {...props} setScrollElement={setScrollElement} body={props.body} />
      )

    return (
      <GraphicalGrid
        {...props}
        setScrollElement={setScrollElement}
        rowCount={model.rowCount}
        columns={grid.columns}
        rowPitch={grid.rowPitch}
        gridClassName={grid.className}
        totalHeight={model.totalHeight}
        rowsToRender={model.rowsToRender}
        onGridKeyDown={model.onGridKeyDown}
        onGridScroll={model.onGridScroll}
      />
    )
  },
)
