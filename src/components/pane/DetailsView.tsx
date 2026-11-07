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
import { entryRenameProps, type InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneState } from '@/types/pane'
import { useLayoutStore } from '@/stores/layout-store'
import { BOTH_SIDES } from '@/lib/numbers'
import { documentZoom } from '@/components/menus/menu-geometry'
import { dropTargetOutline, SCROLLBAR_THIN } from '@/lib/class-names'
import { virtualListHeight, virtualRowsOrFallback } from '@/lib/virtual-rows'
import { MarqueeSelection, type MarqueeRect } from './MarqueeSelection'

/** Rows rendered before the virtualizer has measured its scroll element. */
const UNMEASURED_FALLBACK_ROWS = 30
const FALLBACK_ROW_KEY_PREFIX = 'row'

const PARENT_ROW_ID = '..'
export const DETAILS_ROW_HEIGHT_PX = 28
/** Blank band above the first row and below the last, matching the 3px side inset. */
export const DETAILS_LIST_INSET_PX = 3
const rowHeightPx = DETAILS_ROW_HEIGHT_PX

export type DetailsViewHandle = {
  scrollToRow: (index: number) => void
  getVisibleRowCount: () => number
  contentPoint: (clientX: number, clientY: number) => { x: number; y: number } | null
}

function scrollOverflowClass(usesDetachedMacScrollbars: boolean) {
  return usesDetachedMacScrollbars
    ? 'overflow-x-hidden overflow-y-auto pb-2'
    : 'overflow-x-auto overflow-y-auto'
}

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
    padding: DETAILS_LIST_INSET_PX,
  })
  const itemsToRender = virtualRowsOrFallback({
    measuredRows: rowVirtualizer.getVirtualItems(),
    rowCount,
    fallbackRowLimit: UNMEASURED_FALLBACK_ROWS,
    rowStart: (index) => DETAILS_LIST_INSET_PX + index * rowHeightPx,
    keyPrefix: FALLBACK_ROW_KEY_PREFIX,
  })
  const totalHeight = virtualListHeight({
    measuredHeight: rowVirtualizer.getTotalSize(),
    rowCount,
    estimated: rowCount * rowHeightPx + BOTH_SIDES * DETAILS_LIST_INSET_PX,
  })
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
        const zoom = documentZoom()
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
            className="pointer-events-none absolute bottom-px right-0 top-0 w-2 bg-light-layer dark:bg-dark-layer"
          />
        ) : null}
      </div>
      {body ?? (
        <div className="relative min-h-0 flex-1">
          <div
            ref={parentRef}
            data-testid={`file-pane-scroll-${pane.id}`}
            className={`h-full min-h-0 overscroll-contain ${SCROLLBAR_THIN} ${scrollOverflowClass(usesDetachedMacScrollbars)} ${dropTargetOutline(isPaneDropTarget)}`}
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
                    <div key={PARENT_ROW_ID} className="absolute inset-x-0.75" style={style}>
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
                  <div key={entry.id} className="absolute inset-x-0.75" style={style}>
                    <FileRow
                      entry={entry}
                      isActivePane={isActivePane}
                      isFocused={pane.focusedEntryId === entry.id}
                      isSelected={selectedIds.has(entry.id)}
                      isCut={cutEntryPaths.has(pathKey(entry.path))}
                      isDropTarget={dropTargetEntryId === entry.id}
                      draggable={!entry.trashId}
                      {...entryRenameProps(rename, entry.id)}
                      actions={actions}
                    />
                  </div>
                ) : null
              })}
              <MarqueeSelection rect={marqueeRect} />
            </div>
          </div>
          {usesDetachedMacScrollbars ? (
            <div
              ref={horizontalScrollRef}
              data-testid={`file-pane-horizontal-scroll-${pane.id}`}
              className={`absolute inset-x-0 bottom-0 z-20 h-2 overflow-x-auto overflow-y-hidden overscroll-contain bg-light-layer ${SCROLLBAR_THIN} dark:bg-dark-layer`}
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
