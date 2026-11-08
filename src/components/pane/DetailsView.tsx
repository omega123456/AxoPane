import { forwardRef, useRef } from 'react'
import type { ReactNode, RefObject, UIEvent } from 'react'
import { FileRow, type FileRowActions } from './FileRow'
import { HeaderRow } from './HeaderRow'
import { ParentRow } from './ParentRow'
import { pathKey } from '@/lib/path-compare'
import { entryRenameProps, type InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneState } from '@/types/pane'
import { dropTargetOutline, SCROLLBAR_THIN } from '@/lib/class-names'
import { MarqueeSelection, type MarqueeRect } from './MarqueeSelection'
import {
  useDetachedHorizontalScroll,
  useDetailsHandle,
  useDetailsRows,
  useVisibleContentWidth,
  useVisibleRangeReport,
} from './use-details-view'

const PARENT_ROW_ID = '..'
export { DETAILS_LIST_INSET_PX, DETAILS_ROW_HEIGHT_PX } from './use-details-view'

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

type DetailsRowProps = Pick<
  DetailsViewProps,
  | 'pane'
  | 'isActivePane'
  | 'selectedIds'
  | 'cutEntryPaths'
  | 'dropTargetEntryId'
  | 'rename'
  | 'actions'
  | 'onParentFocus'
  | 'onParentActivate'
> & {
  index: number
  start: number
  hasParent: boolean
  parentOffset: number
}

function DetailsRow(props: DetailsRowProps) {
  const { pane, isActivePane, actions, index, hasParent, parentOffset } = props
  const style = { top: `${props.start}px` }
  if (hasParent && index === 0)
    return (
      <div className="absolute inset-x-0.75" style={style}>
        <ParentRow
          isActivePane={isActivePane}
          isFocused={pane.focusedEntryId === PARENT_ROW_ID}
          onPointerDown={actions.onPointerDown}
          onActivate={props.onParentActivate}
          onFocus={props.onParentFocus}
        />
      </div>
    )
  const entry = pane.entries[index - parentOffset]
  if (!entry) {
    return null
  }
  return (
    <div className="absolute inset-x-0.75" style={style}>
      <FileRow
        entry={entry}
        isActivePane={isActivePane}
        isFocused={pane.focusedEntryId === entry.id}
        isSelected={props.selectedIds.has(entry.id)}
        isCut={props.cutEntryPaths.has(pathKey(entry.path))}
        isDropTarget={props.dropTargetEntryId === entry.id}
        draggable={!entry.trashId}
        {...entryRenameProps(props.rename, entry.id)}
        actions={actions}
      />
    </div>
  )
}

function rowKey(
  virtualRow: { index: number; key: unknown },
  pane: PaneState,
  hasParent: boolean,
  parentOffset: number,
) {
  if (hasParent && virtualRow.index === 0) {
    return PARENT_ROW_ID
  }
  return pane.entries[virtualRow.index - parentOffset]?.id ?? String(virtualRow.key)
}

function DetailsHeader({
  headerScrollRef,
  pane,
  usesDetachedMacScrollbars,
}: {
  headerScrollRef: RefObject<HTMLDivElement | null>
  pane: PaneState
  usesDetachedMacScrollbars: boolean
}) {
  return (
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
  )
}

type DetailsLayout = {
  itemsToRender: Array<{ index: number; start: number; key: unknown }>
  totalHeight: number
  contentWidth: number
  parentOffset: number
}

type DetailsScrollAreaProps = Omit<DetailsViewProps, 'onScroll'> & {
  parentRef: RefObject<HTMLDivElement | null>
  contentLayerRef: RefObject<HTMLDivElement | null>
  onWheel: (event: React.WheelEvent<HTMLDivElement>) => void
  onScroll: (event: UIEvent<HTMLDivElement>) => void
  layout: DetailsLayout
}

function DetailsScrollArea({
  parentRef,
  contentLayerRef,
  onWheel,
  onScroll,
  layout,
  ...props
}: DetailsScrollAreaProps) {
  const { pane, hasParent, usesDetachedMacScrollbars } = props
  const { itemsToRender, totalHeight, contentWidth, parentOffset } = layout
  return (
    <div
      ref={parentRef}
      data-testid={`file-pane-scroll-${pane.id}`}
      className={`h-full min-h-0 overscroll-contain ${SCROLLBAR_THIN} ${scrollOverflowClass(usesDetachedMacScrollbars)} ${dropTargetOutline(props.isPaneDropTarget)}`}
      onMouseDown={props.onContainerMouseDown}
      onWheel={onWheel}
      onScroll={onScroll}
      onContextMenu={props.onContainerContextMenu}
      onDragOver={props.onPaneDragOver}
      onDragLeave={props.onPaneDragLeave}
      onDrop={props.onPaneDrop}
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
        {itemsToRender.map((virtualRow) => (
          <DetailsRow
            key={rowKey(virtualRow, pane, hasParent, parentOffset)}
            {...props}
            index={virtualRow.index}
            start={virtualRow.start}
            parentOffset={parentOffset}
          />
        ))}
        <MarqueeSelection rect={props.marqueeRect} />
      </div>
    </div>
  )
}

type DetachedHorizontalScrollbarProps = {
  scrollRef: RefObject<HTMLDivElement | null>
  paneId: PaneState['id']
  contentWidth: number
  onScrollLeft: (scrollLeft: number) => void
}

function DetachedHorizontalScrollbar({
  scrollRef,
  paneId,
  contentWidth,
  onScrollLeft,
}: DetachedHorizontalScrollbarProps) {
  return (
    <div
      ref={scrollRef}
      data-testid={`file-pane-horizontal-scroll-${paneId}`}
      className={`absolute inset-x-0 bottom-0 z-20 h-2 overflow-x-auto overflow-y-hidden overscroll-contain bg-light-layer ${SCROLLBAR_THIN} dark:bg-dark-layer`}
      onScroll={(event) => onScrollLeft(event.currentTarget.scrollLeft)}
    >
      <div style={{ width: `${contentWidth}px`, height: '1px' }} />
    </div>
  )
}

export const DetailsView = forwardRef<DetailsViewHandle, DetailsViewProps>(
  function DetailsView(props, ref) {
    const { pane, hasParent, usesDetachedMacScrollbars, onScroll } = props
    const internalParentRef = useRef<HTMLDivElement | null>(null)
    const parentRef = props.scrollContainerRef ?? internalParentRef
    const parentOffset = hasParent ? 1 : 0
    const { scrollToIndex, itemsToRender, totalHeight } = useDetailsRows(
      pane.entries.length + parentOffset,
      parentRef,
    )
    const contentWidth = useVisibleContentWidth()
    const {
      headerScrollRef,
      horizontalScrollRef,
      contentLayerRef,
      horizontalScrollLeftRef,
      syncHorizontalScroll,
      handleBodyWheel,
    } = useDetachedHorizontalScroll(usesDetachedMacScrollbars, totalHeight)
    useDetailsHandle(ref, {
      parentRef,
      horizontalScrollLeftRef,
      scrollToIndex,
      usesDetachedMacScrollbars,
    })
    useVisibleRangeReport(itemsToRender, parentOffset, props.onVisibleRangeChange)

    return (
      <>
        <DetailsHeader
          headerScrollRef={headerScrollRef}
          pane={pane}
          usesDetachedMacScrollbars={usesDetachedMacScrollbars}
        />
        {props.body ?? (
          <div className="relative min-h-0 flex-1">
            <DetailsScrollArea
              {...props}
              parentRef={parentRef}
              contentLayerRef={contentLayerRef}
              onWheel={handleBodyWheel}
              onScroll={(event: UIEvent<HTMLDivElement>) => {
                onScroll(event.currentTarget.scrollTop)
                if (!usesDetachedMacScrollbars && headerScrollRef.current)
                  headerScrollRef.current.scrollLeft = event.currentTarget.scrollLeft
              }}
              layout={{ itemsToRender, totalHeight, contentWidth, parentOffset }}
            />
            {usesDetachedMacScrollbars ? (
              <DetachedHorizontalScrollbar
                scrollRef={horizontalScrollRef}
                paneId={pane.id}
                contentWidth={contentWidth}
                onScrollLeft={syncHorizontalScroll}
              />
            ) : null}
          </div>
        )}
      </>
    )
  },
)
