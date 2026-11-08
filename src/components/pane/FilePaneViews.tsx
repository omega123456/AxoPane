import type { DragEvent, MouseEvent, ReactNode, RefObject } from 'react'
import { EmptyState } from '@/components/states/EmptyState'
import { ErrorState } from '@/components/states/ErrorState'
import { LoadingSkeleton } from '@/components/states/LoadingSkeleton'
import { PermissionDenied } from '@/components/states/PermissionDenied'
import { dropTargetOutline, SCROLLBAR_THIN } from '@/lib/class-names'
import type { GraphicalThumbnailRange } from '@/lib/thumbnail-candidates'
import { isTrashPath } from '@/lib/trash'
import type { PaneViewMode } from '@/lib/pane-view'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import type { InlineRenameState } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId, PaneState } from '@/types/pane'
import { DetailsView, type DetailsViewHandle } from './DetailsView'
import type { FileRowActions } from './FileRow'
import { GraphicalSortBar } from './GraphicalSortBar'
import { GraphicalView, type GraphicalViewHandle } from './GraphicalView'
import type { MarqueeRect } from './MarqueeSelection'
import type { PaneBodyState } from './pane-body-state'

type PaneBodyRenderers = Record<PaneBodyState, () => ReactNode>

export type SharedPaneViewProps = {
  pane: PaneState
  isActivePane: boolean
  selectedIds: Set<string>
  cutEntryPaths: Set<string>
  dropTargetEntryId: string | null
  isPaneDropTarget: boolean
  rename: InlineRenameState | null
  actions: FileRowActions
  marqueeRect: MarqueeRect | null
  onVisibleRangeChange: (start: number, end: number, viewportCount: number) => void
  onScroll: (position: number) => void
  onContainerMouseDown: (event: MouseEvent<HTMLDivElement>) => void
  onContainerContextMenu: (event: MouseEvent<HTMLDivElement>) => void
  onPaneDragOver: (event: DragEvent<HTMLDivElement>) => void
  onPaneDragLeave: (event: DragEvent<HTMLDivElement>) => void
  onPaneDrop: (event: DragEvent<HTMLDivElement>) => void
}

type FilePaneViewProps = {
  shared: SharedPaneViewProps
  bodyState: PaneBodyState
  scrollContainerRef: RefObject<HTMLDivElement | null>
}

type FilePaneDetailsProps = FilePaneViewProps & {
  viewRef: RefObject<DetailsViewHandle | null>
  hasParent: boolean
  usesDetachedMacScrollbars: boolean
  onParentFocus: () => void
  onParentActivate: (eventTimeStamp?: number) => void
}

type FilePaneGraphicalProps = FilePaneViewProps & {
  viewRef: RefObject<GraphicalViewHandle | null>
  mode: Exclude<PaneViewMode, 'details'>
  onThumbnailRangeChange: (range: GraphicalThumbnailRange) => void
  onFocusEntry: (entryId: string) => void
}

function newFolderActionFor(paneId: PaneId, path: string) {
  if (isTrashPath(path)) {
    return undefined
  }
  return () => useActionDialogStore.getState().open({ kind: 'newFolder', paneId })
}

function sharedBodyRenderers(pane: PaneState) {
  const retryListing = () => void usePanesStore.getState().reloadPane(pane.id)
  return {
    permissionDenied: () => <PermissionDenied />,
    error: () => <ErrorState path={pane.path} message={pane.error ?? ''} onRetry={retryListing} />,
    listing: () => undefined,
  }
}

function detailsBodyRenderers(shared: SharedPaneViewProps): PaneBodyRenderers {
  return {
    ...sharedBodyRenderers(shared.pane),
    skeleton: () => (
      <div
        className={`min-h-0 flex-1 overflow-x-auto overflow-y-auto overscroll-contain ${SCROLLBAR_THIN}`}
      >
        <LoadingSkeleton />
      </div>
    ),
    empty: () => (
      <div
        className={`flex min-h-0 flex-1 flex-col ${dropTargetOutline(shared.isPaneDropTarget)}`}
        onDragOver={shared.onPaneDragOver}
        onDragLeave={shared.onPaneDragLeave}
        onDrop={shared.onPaneDrop}
      >
        <EmptyState onNewFolder={newFolderActionFor(shared.pane.id, shared.pane.path)} />
      </div>
    ),
  }
}

function graphicalBodyRenderers(pane: PaneState): PaneBodyRenderers {
  return {
    ...sharedBodyRenderers(pane),
    skeleton: () => (
      <div className="min-h-0 flex-1 overflow-auto">
        <LoadingSkeleton />
      </div>
    ),
    empty: () => (
      <div className="flex min-h-0 flex-1 flex-col">
        <EmptyState onNewFolder={newFolderActionFor(pane.id, pane.path)} />
      </div>
    ),
  }
}

export function FilePaneDetails({ shared, bodyState, viewRef, ...props }: FilePaneDetailsProps) {
  return (
    <DetailsView
      ref={viewRef}
      {...shared}
      hasParent={props.hasParent}
      usesDetachedMacScrollbars={props.usesDetachedMacScrollbars}
      onParentFocus={props.onParentFocus}
      onParentActivate={props.onParentActivate}
      scrollContainerRef={props.scrollContainerRef}
      body={detailsBodyRenderers(shared)[bodyState]()}
    />
  )
}

export function FilePaneGraphical({
  shared,
  bodyState,
  viewRef,
  ...props
}: FilePaneGraphicalProps) {
  return (
    <>
      <GraphicalSortBar pane={shared.pane} />
      <GraphicalView
        ref={viewRef}
        {...shared}
        mode={props.mode}
        onThumbnailRangeChange={props.onThumbnailRangeChange}
        onFocusEntry={props.onFocusEntry}
        scrollContainerRef={props.scrollContainerRef}
        body={graphicalBodyRenderers(shared.pane)[bodyState]()}
      />
    </>
  )
}
