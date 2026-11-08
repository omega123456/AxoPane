import { useMemo, useRef, type MouseEvent, type RefObject } from 'react'
import { detectPlatformOs, type PlatformOs } from '@/lib/keymap'
import { pathKey } from '@/lib/path-compare'
import { useDelayedFlag } from '@/lib/use-delayed-flag'
import { useModalOpen } from '@/lib/use-modal-open'
import type { KeyHandlerTable } from '@/lib/key-table'
import type { PaneViewMode } from '@/lib/pane-view'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { getParentPath, usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneId, PaneState } from '@/types/pane'
import type { DetailsViewHandle } from './DetailsView'
import type { GraphicalViewHandle } from './GraphicalView'
import type { MarqueeRect } from './MarqueeSelection'
import type { SharedPaneViewProps } from './FilePaneViews'
import { DETAILS_LIST_INSET_PX, DETAILS_ROW_HEIGHT_PX } from './DetailsView'
import { selectionForClick } from './click-selection'
import { detailsEntryIdsInRect } from './marquee-selection'
import { paneBodyState } from './pane-body-state'
import { gridMovementKeyTable, rowNavigationKeyTable } from './pane-keyboard'
import { usePaneFocus, type PaneRowLayout } from './use-pane-focus'
import { usePaneKeyboard } from './use-pane-keyboard'
import { useMarqueeSelection } from './use-marquee-selection'
import { useScrollRestore } from './use-scroll-restore'
import { useVisibleRangeRequests } from './use-visible-range-requests'
import { usePaneThumbnails } from './use-pane-thumbnails'
import { usePaneDragSource } from './use-pane-drag-source'
import { usePaneContextMenu } from './use-pane-context-menu'
import { useInlineRenameHandlers } from './use-inline-rename-handlers'
import { useStableRowActions } from './use-stable-row-actions'

const SKELETON_DELAY_MS = 1000

export type PaneContext = {
  paneId: PaneId
  pane: PaneState
  viewMode: PaneViewMode
  layout: PaneRowLayout
  platform: PlatformOs
  selectedIds: string[]
  isActivePane: boolean
}

function paneRowLayout(pane: PaneState): PaneRowLayout {
  const hasParent = getParentPath(pane.path) !== null
  const parentOffset = hasParent ? 1 : 0
  return { hasParent, parentOffset, rowCount: pane.entries.length + parentOffset }
}

export function usePaneRefs() {
  return {
    paneRef: useRef<HTMLElement | null>(null),
    scrollContainerRef: useRef<HTMLDivElement | null>(null),
    detailsViewRef: useRef<DetailsViewHandle | null>(null),
    graphicalViewRef: useRef<GraphicalViewHandle | null>(null),
  }
}

export type PaneRefs = ReturnType<typeof usePaneRefs>

export function usePaneShellFocus(paneId: PaneId, paneRef: RefObject<HTMLElement | null>) {
  const setActivePane = usePanesStore((state) => state.setActivePane)
  function focusPaneShell() {
    setActivePane(paneId)
    paneRef.current?.focus()
  }
  return { setActivePane, focusPaneShell }
}

export function usePaneContext(paneId: PaneId) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const activeTab = useTabsStore((state) => {
    const tabs = state.panes[paneId]
    return tabs.tabs[tabs.activeTabIndex]
  })
  const selectedIds = useSelectionStore((state) => state.selections[paneId].selectedIds)
  const isActivePane = usePanesStore((state) => state.activePaneId === paneId)
  const context: PaneContext = {
    paneId,
    pane,
    viewMode: activeTab.viewMode,
    layout: paneRowLayout(pane),
    platform: detectPlatformOs(),
    selectedIds,
    isActivePane,
  }
  return { context, activeTabId: activeTab.id }
}

export function useCutEntryPaths() {
  const clipboardMode = useClipboardStore((state) => state.mode)
  const clipboardEntries = useClipboardStore((state) => state.entries)
  return useMemo(() => {
    if (clipboardMode !== 'move') {
      return new Set<string>()
    }
    return new Set(clipboardEntries.map((entry) => pathKey(entry.path)))
  }, [clipboardEntries, clipboardMode])
}

export function usePaneBodyState({ pane, layout }: PaneContext) {
  const showSkeleton = useDelayedFlag(pane.loading, SKELETON_DELAY_MS)
  return paneBodyState({
    showSkeleton,
    loading: pane.loading,
    error: pane.error,
    rowCount: layout.rowCount,
  })
}

export function useClickSelection({ paneId, pane }: PaneContext) {
  const selection = useSelectionStore((state) => state.selections[paneId])
  const setSelection = useSelectionStore((state) => state.setSelection)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  function selectWithModifiers(entryId: string, event: MouseEvent<HTMLDivElement>) {
    const update = selectionForClick(pane.entries, selection, entryId, event)
    setSelection(paneId, update.selectedIds, update.anchorId, update.focusedId)
    setFocusedEntry(paneId, entryId)
  }
  function selectSingleEntry(entryId: string) {
    setFocusedEntry(paneId, entryId)
    setSelection(paneId, [entryId], entryId, entryId)
  }
  return { selectWithModifiers, selectSingleEntry }
}

export function useSelectedIdSet(selectedIds: string[]) {
  return useMemo(() => new Set(selectedIds), [selectedIds])
}

export function usePaneMarquee(context: PaneContext, refs: PaneRefs, focusPaneShell: () => void) {
  const { paneId, pane, viewMode, layout } = context
  function entryIdsInMarquee(rect: MarqueeRect) {
    if (viewMode !== 'details') {
      return refs.graphicalViewRef.current?.getEntriesIntersectingRect(rect) ?? []
    }
    return detailsEntryIdsInRect(rect, pane.entries, {
      listInsetPx: DETAILS_LIST_INSET_PX,
      rowHeightPx: DETAILS_ROW_HEIGHT_PX,
      rowCount: layout.rowCount,
      parentOffset: layout.parentOffset,
    })
  }
  const setSelection = useSelectionStore((state) => state.setSelection)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  return useMarqueeSelection({
    paneId,
    scrollContainerRef: refs.scrollContainerRef,
    selectedIds: context.selectedIds,
    entryIdsInRect: entryIdsInMarquee,
    onPointerDown: focusPaneShell,
    setSelection,
    setFocusedEntry,
  })
}

export function useFilePaneKeyboard(
  context: PaneContext,
  refs: PaneRefs,
  openFromFilter: (entryId: string) => void,
) {
  const { paneId, pane, viewMode, layout } = context
  const modalOpen = useModalOpen()
  const keymap = useKeymapStore((state) => state.bindings)
  const focus = usePaneFocus({ ...refs, paneId, pane, layout, viewMode })

  function navigationKeyTable(): KeyHandlerTable {
    const rowNavigation = rowNavigationKeyTable({
      focusedRowIndex: focus.focusedRowIndex,
      rowCount: layout.rowCount,
      visibleRowCount: focus.visibleRowCount,
      focusRow: focus.focusByRowIndex,
    })
    if (viewMode === 'details') {
      return rowNavigation
    }
    const moveGridFocus = gridMovementKeyTable((movement) =>
      refs.graphicalViewRef.current?.moveFocus(movement),
    )
    return { ...rowNavigation, ...moveGridFocus }
  }

  const handlePaneKeyDown = usePaneKeyboard({
    paneId,
    acceptsKeys: context.isActivePane && !modalOpen,
    keymap,
    focusedEntryId: pane.focusedEntryId,
    navigationKeyTable,
    openFromFilter,
  })
  return { handlePaneKeyDown, focusByRowIndex: focus.focusByRowIndex }
}

export function usePaneDataRequests(context: PaneContext, activeTabId: string, refs: PaneRefs) {
  const { paneId, pane, viewMode } = context
  const setScrollPosition = usePanesStore((state) => state.setScrollPosition)
  const recordScroll = useScrollRestore({
    paneId,
    path: pane.path,
    savedScrollTop: pane.scrollPositions[pane.path] ?? 0,
    entryCount: pane.entries.length,
    scrollContainerRef: refs.scrollContainerRef,
    setScrollPosition,
  })
  const handleVisibleRangeChange = useVisibleRangeRequests({
    paneId,
    path: pane.path,
    entries: pane.entries,
  })
  const handleThumbnailRangeChange = usePaneThumbnails({
    paneId,
    tabId: activeTabId,
    path: pane.path,
    mode: viewMode,
    entries: pane.entries,
  })
  return { recordScroll, handleVisibleRangeChange, handleThumbnailRangeChange }
}

type RowInteractionSources = {
  context: PaneContext
  focusPaneShell: () => void
  activateEntryFromPointer: (entryId: string, clickCount?: number, eventTimeStamp?: number) => void
}

export function usePaneRowInteractions({
  context,
  focusPaneShell,
  activateEntryFromPointer,
}: RowInteractionSources) {
  const { paneId, pane, platform, selectedIds } = context
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const drag = usePaneDragSource({
    paneId,
    path: pane.path,
    entries: pane.entries,
    selectedIds,
    platform,
  })
  const showMenu = usePaneContextMenu({ paneId, entries: pane.entries, selectedIds, platform })
  const { selectWithModifiers, selectSingleEntry } = useClickSelection(context)
  const renameHandlers = useInlineRenameHandlers(paneId, focusPaneShell)

  function middleClickOpen(entryId: string) {
    const entry = pane.entries.find((candidate) => candidate.id === entryId)
    if (entry) void openTabFromPath(paneId, entry.path)
  }

  const actions = useStableRowActions({
    onPointerDown: focusPaneShell,
    onActivate: activateEntryFromPointer,
    onClick: selectWithModifiers,
    onContextMenu: (entryId, event) => showMenu(event, entryId),
    onMiddleClick: middleClickOpen,
    onDragStart: drag.handleRowDragStart,
    onDragEnd: drag.clearDragState,
    onDragEnter: drag.handleFolderDragOver,
    onDragOver: drag.handleFolderDragOver,
    onDragLeave: drag.handleFolderDragLeave,
    onDrop: (entryId, event) => void drag.handleFolderDrop(entryId, event),
    onRenameChange: renameHandlers.setRenameValue,
    onRenameSubmit: () => void renameHandlers.submitRename(),
    onRenameCancel: renameHandlers.renameCancel,
    onRenameBlur: renameHandlers.renameBlur,
  })
  return { actions, drag, showMenu, selectSingleEntry, rename: renameHandlers.rename }
}

type SharedViewSources = {
  context: PaneContext
  requests: ReturnType<typeof usePaneDataRequests>
  marquee: ReturnType<typeof usePaneMarquee>
  row: ReturnType<typeof usePaneRowInteractions>
}

export function useSharedViewProps(sources: SharedViewSources): SharedPaneViewProps {
  const { context, requests, marquee, row } = sources
  const selectedIdSet = useSelectedIdSet(context.selectedIds)
  const cutEntryPaths = useCutEntryPaths()
  return {
    pane: context.pane,
    isActivePane: context.isActivePane,
    selectedIds: selectedIdSet,
    cutEntryPaths,
    dropTargetEntryId: row.drag.dropTargetEntryId,
    isPaneDropTarget: row.drag.isPaneDropTarget,
    rename: row.rename,
    actions: row.actions,
    marqueeRect: marquee.marqueeRect,
    onVisibleRangeChange: requests.handleVisibleRangeChange,
    onScroll: requests.recordScroll,
    onContainerMouseDown: marquee.handleContainerMouseDown,
    onContainerContextMenu: (event) => row.showMenu(event),
    onPaneDragOver: row.drag.handlePaneDragOver,
    onPaneDragLeave: row.drag.handlePaneDragLeave,
    onPaneDrop: (event) => void row.drag.handlePaneDrop(event),
  }
}
