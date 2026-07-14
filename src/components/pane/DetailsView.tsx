import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useLayoutEffect,
  useMemo,
  useRef,
} from 'react'
import type { ReactNode, RefObject, UIEvent } from 'react'
import { FileRow, type FileRowActions } from './FileRow'
import { HeaderRow, paneContentWidth } from './HeaderRow'
import { ParentRow } from './ParentRow'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { pathKey } from '@/lib/path-compare'
import type { InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneState } from '@/types/pane'
import { useLayoutStore } from '@/stores/layout-store'

const PARENT_ROW_ID = '..'
const rowHeightPx = 30

export type DetailsViewHandle = {
  scrollToRow: (index: number) => void
  getVisibleRowCount: () => number
  contentPoint: (clientX: number, clientY: number) => { x: number; y: number } | null
}

type MarqueeRect = { left: number; top: number; width: number; height: number }

type DetailsViewProps = {
  pane: PaneState
  isActivePane: boolean
  hasParent: boolean
  selectedIds: Set<string>
  cutEntryPaths: Set<string>
  dropTargetEntryId: string | null
  isPaneDropTarget: boolean
  rename: InlineRenameState | null
  actions: FileRowActions
  usesDetachedMacScrollbars: boolean
  marqueeRect: MarqueeRect | null
  onVisibleRangeChange: (start: number, end: number, viewportCount: number) => void
  onContainerMouseDown: (event: React.MouseEvent<HTMLDivElement>) => void
  onScroll: (position: number) => void
  onContainerContextMenu: (event: React.MouseEvent<HTMLDivElement>) => void
  onPaneDragOver: (event: React.DragEvent<HTMLDivElement>) => void
  onPaneDragLeave: (event: React.DragEvent<HTMLDivElement>) => void
  onPaneDrop: (event: React.DragEvent<HTMLDivElement>) => void
  onParentFocus: () => void
  onParentActivate: (eventTimeStamp?: number) => void
  body?: ReactNode
  scrollContainerRef?: RefObject<HTMLDivElement | null>
}

export const DetailsView = forwardRef<DetailsViewHandle, DetailsViewProps>(function DetailsView(
  {
    pane,
    isActivePane,
    hasParent,
    selectedIds,
    cutEntryPaths,
    dropTargetEntryId,
    isPaneDropTarget,
    rename,
    actions,
    usesDetachedMacScrollbars,
    marqueeRect,
    onVisibleRangeChange,
    onContainerMouseDown,
    onScroll,
    onContainerContextMenu,
    onPaneDragOver,
    onPaneDragLeave,
    onPaneDrop,
    onParentFocus,
    onParentActivate,
    body,
    scrollContainerRef,
  },
  ref,
) {
  const internalParentRef = useRef<HTMLDivElement | null>(null)
  const internalHeaderScrollRef = useRef<HTMLDivElement | null>(null)
  const internalHorizontalScrollRef = useRef<HTMLDivElement | null>(null)
  const internalContentLayerRef = useRef<HTMLDivElement | null>(null)
  const parentRef = scrollContainerRef ?? internalParentRef
  const headerScrollRef = internalHeaderScrollRef
  const horizontalScrollRef = internalHorizontalScrollRef
  const contentLayerRef = internalContentLayerRef
  const horizontalScrollLeftRef = useRef(0)
  const parentOffset = hasParent ? 1 : 0
  const rowCount = pane.entries.length + parentOffset
  const rowVirtualizer = useElementVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeightPx,
    overscan: 10,
  })
  const virtualItems = rowVirtualizer.getVirtualItems()
  const itemsToRender =
    virtualItems.length > 0 || rowCount === 0
      ? virtualItems
      : Array.from({ length: Math.min(rowCount, 30) }, (_, index) => ({
          key: `row-${index}`,
          index,
          start: index * rowHeightPx,
        }))
  const totalHeight = rowCount === 0 ? 0 : rowVirtualizer.getTotalSize() || rowCount * rowHeightPx
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
  const contentWidth = useMemo(
    () =>
      paneContentWidth(
        configuredColumns.filter((column) => column.visible),
        columnWidths,
      ),
    [columnWidths, configuredColumns],
  )

  useImperativeHandle(
    ref,
    () => ({
      scrollToRow: (index) => rowVirtualizer.scrollToIndex(index),
      getVisibleRowCount: () =>
        Math.max(1, Math.floor((parentRef.current?.clientHeight ?? 0) / rowHeightPx)),
      contentPoint: (clientX, clientY) => {
        const container = parentRef.current
        if (!container) return null
        const bounds = container.getBoundingClientRect()
        const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
        return {
          x:
            (clientX - bounds.left) / zoom +
            (usesDetachedMacScrollbars ? horizontalScrollLeftRef.current : container.scrollLeft),
          y: (clientY - bounds.top) / zoom + container.scrollTop,
        }
      },
    }),
    [parentRef, rowVirtualizer, usesDetachedMacScrollbars],
  )

  useEffect(() => {
    if (itemsToRender.length === 0) return
    const start = Math.max(0, itemsToRender[0].index - parentOffset)
    const end = Math.max(0, itemsToRender[itemsToRender.length - 1].index - parentOffset)
    onVisibleRangeChange(start, end, Math.max(1, end - start + 1))
  }, [itemsToRender, onVisibleRangeChange, parentOffset])

  const syncHorizontalScroll = useCallback(
    (scrollLeft: number) => {
      horizontalScrollLeftRef.current = scrollLeft
      if (headerScrollRef.current) headerScrollRef.current.scrollLeft = scrollLeft
      if (contentLayerRef.current)
        contentLayerRef.current.style.transform =
          scrollLeft === 0 ? '' : `translateX(-${scrollLeft}px)`
    },
    [contentLayerRef, headerScrollRef],
  )
  function handleBodyWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (!usesDetachedMacScrollbars || event.deltaX === 0) return

    const horizontalScroll = horizontalScrollRef.current
    if (!horizontalScroll) return

    const maxScrollLeft = Math.max(0, horizontalScroll.scrollWidth - horizontalScroll.clientWidth)
    const nextScrollLeft = Math.max(
      0,
      Math.min(maxScrollLeft, horizontalScroll.scrollLeft + event.deltaX),
    )
    if (nextScrollLeft !== horizontalScroll.scrollLeft) {
      horizontalScroll.scrollLeft = nextScrollLeft
      syncHorizontalScroll(nextScrollLeft)
    }
  }
  useLayoutEffect(() => {
    if (!usesDetachedMacScrollbars) {
      horizontalScrollLeftRef.current = 0
      if (contentLayerRef.current) contentLayerRef.current.style.transform = ''
      return
    }
    syncHorizontalScroll(horizontalScrollRef.current?.scrollLeft ?? 0)
  }, [
    contentLayerRef,
    horizontalScrollRef,
    syncHorizontalScroll,
    totalHeight,
    usesDetachedMacScrollbars,
  ])

  return (
    <>
      <div
        ref={headerScrollRef}
        data-testid={`file-pane-header-scroll-${pane.id}`}
        className="relative overflow-hidden"
      >
        <HeaderRow pane={pane} />
        {usesDetachedMacScrollbars ? (
          <div
            aria-hidden="true"
            className="pointer-events-none absolute inset-y-0 right-0 w-2 bg-light-header dark:bg-dark-header"
          />
        ) : null}
      </div>
      {body ?? (
        <div className="relative min-h-0 flex-1">
          <div
            ref={parentRef}
            data-testid={`file-pane-scroll-${pane.id}`}
            className={`h-full min-h-0 overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint ${usesDetachedMacScrollbars ? 'overflow-x-hidden overflow-y-auto pb-2' : 'overflow-x-auto overflow-y-auto'} ${isPaneDropTarget ? 'outline outline-2 -outline-offset-2 outline-accent-blue-border' : ''}`}
            onMouseDown={onContainerMouseDown}
            onWheel={handleBodyWheel}
            onScroll={(event: UIEvent<HTMLDivElement>) => {
              onScroll(event.currentTarget.scrollTop)
              if (!usesDetachedMacScrollbars && headerScrollRef.current)
                headerScrollRef.current.scrollLeft = event.currentTarget.scrollLeft
            }}
            onContextMenu={onContainerContextMenu}
            onDragOver={onPaneDragOver}
            onDragLeave={onPaneDragLeave}
            onDrop={onPaneDrop}
          >
            <div
              ref={contentLayerRef}
              style={{
                height: `${totalHeight}px`,
                minWidth: `${contentWidth}px`,
                position: 'relative',
              }}
              className={`min-h-full w-full ${usesDetachedMacScrollbars ? 'will-change-transform' : ''}`}
            >
              {itemsToRender.map((virtualRow) => {
                const style = { top: `${virtualRow.start}px` }
                if (hasParent && virtualRow.index === 0)
                  return (
                    <div key={PARENT_ROW_ID} className="absolute inset-x-0" style={style}>
                      <ParentRow
                        isActivePane={isActivePane}
                        isFocused={pane.focusedEntryId === PARENT_ROW_ID}
                        onPointerDown={actions.onPointerDown}
                        onActivate={onParentActivate}
                        onFocus={onParentFocus}
                      />
                    </div>
                  )
                const entry = pane.entries[virtualRow.index - parentOffset]
                return entry ? (
                  <div key={entry.id} className="absolute inset-x-0" style={style}>
                    <FileRow
                      entry={entry}
                      isActivePane={isActivePane}
                      isFocused={pane.focusedEntryId === entry.id}
                      isSelected={selectedIds.has(entry.id)}
                      isCut={cutEntryPaths.has(pathKey(entry.path))}
                      isDropTarget={dropTargetEntryId === entry.id}
                      draggable={!entry.trashId}
                      isRenaming={rename?.entryId === entry.id}
                      renameValue={rename?.entryId === entry.id ? rename.value : ''}
                      renameBusy={rename?.entryId === entry.id ? rename.busy : false}
                      renameError={rename?.entryId === entry.id ? rename.error : null}
                      actions={actions}
                    />
                  </div>
                ) : null
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
          {usesDetachedMacScrollbars ? (
            <div
              ref={horizontalScrollRef}
              data-testid={`file-pane-horizontal-scroll-${pane.id}`}
              className="absolute inset-x-0 bottom-0 z-20 h-2 overflow-x-auto overflow-y-hidden overscroll-contain bg-light-surface scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:bg-dark-surface dark:scrollbar-thumb-dark-text-faint"
              onScroll={(event) => syncHorizontalScroll(event.currentTarget.scrollLeft)}
            >
              <div style={{ width: `${contentWidth}px`, height: '1px' }} />
            </div>
          ) : null}
        </div>
      )}
    </>
  )
})
