import { useCallback, useEffect, useLayoutEffect, useMemo, useRef } from 'react'
import {
  buildContextMenuContent,
  describeMenuTarget,
  resolveMenuTarget,
} from '@/components/menus/menu-definitions'
import { BreadcrumbBar } from './BreadcrumbBar'
import { type FileRowActions } from './FileRow'
import { DETAILS_LIST_INSET_PX, DETAILS_ROW_HEIGHT_PX, DetailsView } from './DetailsView'
import { GraphicalView, type GraphicalViewHandle } from './GraphicalView'
import { paneBodyState, type PaneBodyState } from './pane-body-state'
import { GraphicalSortBar } from './GraphicalSortBar'
import { executeCommand } from '@/lib/commands'
import { detectPlatformOs } from '@/lib/keymap'
import { getParentPath } from '@/stores/panes-store'
import { pathKey } from '@/lib/path-compare'
import { isTrashPath } from '@/lib/trash'
import type { PaneViewMode } from '@/lib/pane-view'
import { EmptyState } from '@/components/states/EmptyState'
import { ErrorState } from '@/components/states/ErrorState'
import { LoadingSkeleton } from '@/components/states/LoadingSkeleton'
import { PermissionDenied } from '@/components/states/PermissionDenied'
import { useDelayedFlag } from '@/lib/use-delayed-flag'
import { renameEntryInPane } from '@/lib/file-actions'
import { log } from '@/lib/app-log-commands'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useNativeMenuWarmStore } from '@/stores/native-menu-warm-store'
import { useThumbnailStore } from '@/stores/thumbnail-store'
import { useTabsStore } from '@/stores/tabs-store'
import {
  thumbnailCandidatesInRange,
  type GraphicalThumbnailRange,
} from '@/lib/thumbnail-candidates'
import type { KeyHandlerTable } from '@/lib/key-table'
import { useModalOpen } from '@/lib/use-modal-open'
import type { PaneId } from '@/types/pane'
import { clamp } from '@/lib/numbers'
import { dropTargetOutline, SCROLLBAR_THIN } from '@/lib/class-names'
import type { MarqueeRect } from './MarqueeSelection'
import { detailsEntryIdsInRect } from './marquee-selection'
import { gridMovementKeyTable, rowNavigationKeyTable } from './pane-keyboard'
import { useScrollRestore } from './use-scroll-restore'
import { useMarqueeSelection } from './use-marquee-selection'
import { usePaneDragSource } from './use-pane-drag-source'
import { usePaneKeyboard } from './use-pane-keyboard'

const SKELETON_DELAY_MS = 1000
const DOUBLE_CLICK_COUNT = 2

const PARENT_ROW_ID = '..'
// ponytail: 100 ms separates generated click bursts; tune only for measured slower macros.
const macroContinuationWindowMs = 100
// Parent-row activation pairs raw clicks itself instead of trusting dblclick:
// webviews keep the OS click count running across up-navigations on the same
// spot, so a second double-click on ".." can arrive as detail 3/4 (or no
// dblclick at all). 500 ms matches the default OS double-click interval.
const parentClickPairWindowMs = 500
const visibleIconRequestDebounceMs = 100

type PaneBodyRenderers = Record<PaneBodyState, () => React.ReactNode>

type FilePaneProps = {
  paneId: PaneId
}

export function FilePane({ paneId }: FilePaneProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const activeTab = useTabsStore((state) => {
    const tabs = state.panes[paneId]
    return tabs.tabs[tabs.activeTabIndex]
  })
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const focusRequestId = usePanesStore((state) => state.focusRequestId)
  const focusRequestPaneId = usePanesStore((state) => state.focusRequestPaneId)
  const revealScrollRequestId = usePanesStore((state) => state.revealScrollRequests[paneId])
  const setActivePane = usePanesStore((state) => state.setActivePane)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const requestVisibleItemCounts = usePanesStore((state) => state.requestVisibleItemCounts)
  const requestVisibleIcons = usePanesStore((state) => state.requestVisibleIcons)
  const warmVisibleNativeMenus = useNativeMenuWarmStore((state) => state.warmVisibleNativeMenus)
  const setThumbnailCandidates = useThumbnailStore((state) => state.setVisibleCandidates)
  const cancelThumbnailScope = useThumbnailStore((state) => state.cancelScope)
  const setScrollPosition = usePanesStore((state) => state.setScrollPosition)
  const selection = useSelectionStore((state) => state.selections[paneId])
  const keymap = useKeymapStore((state) => state.bindings)
  const setSelection = useSelectionStore((state) => state.setSelection)
  const clipboardMode = useClipboardStore((state) => state.mode)
  const clipboardEntries = useClipboardStore((state) => state.entries)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  // A pane keeps its own keydown handler (it runs before the window-level
  // fallback in App.tsx). Suppress pane shortcuts while any app-modal surface is
  // open so a confirmation dialog blocks the whole app, not just the inactive
  // pane.
  const modalOpen = useModalOpen()
  const rename = useInlineRenameStore((state) =>
    state.rename?.paneId === paneId ? state.rename : null,
  )
  const cancelRename = useInlineRenameStore((state) => state.cancelRename)
  const setRenameBusy = useInlineRenameStore((state) => state.setBusy)
  const setRenameError = useInlineRenameStore((state) => state.setError)
  const setRenameValue = useInlineRenameStore((state) => state.setValue)
  const paneRef = useRef<HTMLElement | null>(null)
  const parentRef = useRef<HTMLDivElement | null>(null)
  const detailsViewRef = useRef<React.ElementRef<typeof DetailsView>>(null)
  const graphicalViewRef = useRef<GraphicalViewHandle>(null)
  const savedScrollTop = pane.scrollPositions[pane.path] ?? 0
  const lastPointerActivationAtRef = useRef<number | null>(null)
  const renameSubmittingRef = useRef(false)
  const ignoreNextRenameBlurRef = useRef(false)
  const visibleIconRequestTimerRef = useRef<number | undefined>(undefined)
  const thumbnailRequestFrameRef = useRef<number | undefined>(undefined)
  const pendingThumbnailRangeRef = useRef<{
    range: GraphicalThumbnailRange
    entries: typeof pane.entries
    paneId: string
    tabId: string
    path: string
    mode: PaneViewMode
  } | null>(null)
  const activeTabIdRef = useRef(activeTab.id)
  const thumbnailPathRef = useRef(pane.path)
  const isActivePane = activePaneId === paneId
  const platform = detectPlatformOs()
  const usesDetachedMacScrollbars = platform === 'macos'
  // Suppress the loading skeleton on fast loads, avoiding a jarring
  // flash-and-replace when a folder resolves in a few milliseconds.
  const showSkeleton = useDelayedFlag(pane.loading, SKELETON_DELAY_MS)

  const hasParent = getParentPath(pane.path) !== null
  const parentOffset = hasParent ? 1 : 0
  const rowCount = pane.entries.length + parentOffset
  const cutEntryPaths = useMemo(
    () =>
      clipboardMode === 'move'
        ? new Set(clipboardEntries.map((entry) => pathKey(entry.path)))
        : new Set<string>(),
    [clipboardEntries, clipboardMode],
  )
  // O(1) per-row membership check instead of `selectedIds.includes(...)`
  // (O(selection size)) on every row on every render.
  const selectedIdSet = useMemo(() => new Set(selection.selectedIds), [selection.selectedIds])

  const viewMode = activeTab.viewMode

  useEffect(() => {
    if (focusRequestId > 0 && focusRequestPaneId === paneId) {
      paneRef.current?.focus()
    }
  }, [focusRequestId, focusRequestPaneId, paneId])

  const recordScroll = useScrollRestore({
    paneId,
    path: pane.path,
    savedScrollTop,
    entryCount: pane.entries.length,
    scrollContainerRef: parentRef,
    setScrollPosition,
  })

  const handleVisibleRangeChange = useCallback(
    (start: number, end: number, viewportCount: number) => {
      window.clearTimeout(visibleIconRequestTimerRef.current)
      visibleIconRequestTimerRef.current = window.setTimeout(() => {
        if (!isTrashPath(pane.path))
          void requestVisibleItemCounts(paneId, start, end, viewportCount)
        const visiblePaths = pane.entries.slice(start, end + 1).map((entry) => entry.path)
        void requestVisibleIcons(paneId, visiblePaths)
        void warmVisibleNativeMenus(paneId, visiblePaths)
      }, visibleIconRequestDebounceMs)
    },
    [
      pane.entries,
      pane.path,
      paneId,
      requestVisibleIcons,
      requestVisibleItemCounts,
      warmVisibleNativeMenus,
    ],
  )

  const handleThumbnailRangeChange = useCallback(
    (range: GraphicalThumbnailRange) => {
      pendingThumbnailRangeRef.current = {
        range,
        entries: pane.entries,
        paneId,
        tabId: activeTab.id,
        path: pane.path,
        mode: viewMode,
      }
      if (thumbnailRequestFrameRef.current !== undefined) return
      thumbnailRequestFrameRef.current = window.requestAnimationFrame(() => {
        thumbnailRequestFrameRef.current = undefined
        const latest = pendingThumbnailRangeRef.current
        if (!latest || latest.mode !== 'thumbnails') return
        const candidates = thumbnailCandidatesInRange(latest.entries, latest.range)
        void setThumbnailCandidates(
          {
            paneId: latest.paneId,
            tabId: latest.tabId,
            path: latest.path,
            mode: latest.mode,
          },
          candidates,
        ).catch(() => undefined)
      })
    },
    [activeTab.id, pane.entries, pane.path, paneId, setThumbnailCandidates, viewMode],
  )

  useEffect(
    () => () => {
      window.clearTimeout(visibleIconRequestTimerRef.current)
      if (thumbnailRequestFrameRef.current !== undefined)
        window.cancelAnimationFrame(thumbnailRequestFrameRef.current)
    },
    [],
  )

  useEffect(() => {
    const previousTabId = activeTabIdRef.current
    const previousPath = thumbnailPathRef.current
    activeTabIdRef.current = activeTab.id
    thumbnailPathRef.current = pane.path
    if (viewMode !== 'thumbnails' || previousPath !== pane.path) {
      void cancelThumbnailScope(paneId, activeTab.id)
    }
    if (previousTabId !== activeTab.id) {
      void cancelThumbnailScope(paneId, previousTabId)
    }
  }, [activeTab.id, cancelThumbnailScope, pane.path, paneId, viewMode])

  useEffect(
    () => () => {
      void cancelThumbnailScope(paneId, activeTabIdRef.current)
    },
    [cancelThumbnailScope, paneId],
  )

  // Row indices include the synthetic parent row at position 0 when present.
  // Row 0 (with a parent) is the parent row; entry `i` is at row `i + parentOffset`.
  const focusedRowIndex = useMemo(() => {
    if (hasParent && pane.focusedEntryId === PARENT_ROW_ID) {
      return 0
    }
    const entryIndex = pane.entries.findIndex((entry) => entry.id === pane.focusedEntryId)
    return entryIndex < 0 ? -1 : entryIndex + parentOffset
  }, [hasParent, parentOffset, pane.entries, pane.focusedEntryId])

  // Navigating up highlights the folder we came from (see the store's
  // parent-reveal handling); make sure that row is actually scrolled into
  // view — a fresh up-navigation starts the parent listing at the top, where
  // the highlighted folder may live below the fold.
  const handledRevealScrollIdRef = useRef<number | undefined>(undefined)
  useEffect(() => {
    if (
      revealScrollRequestId === undefined ||
      revealScrollRequestId === handledRevealScrollIdRef.current ||
      focusedRowIndex < 0
    ) {
      return
    }
    handledRevealScrollIdRef.current = revealScrollRequestId
    if (viewMode === 'details') detailsViewRef.current?.scrollToRow(focusedRowIndex)
    else graphicalViewRef.current?.scrollToEntry(Math.max(0, focusedRowIndex - parentOffset))
  }, [focusedRowIndex, parentOffset, revealScrollRequestId, viewMode])

  // Rows fully visible in the scroll viewport, used as the Page Up/Down step —
  // matches Explorer's "jump by a screenful" behavior instead of a fixed count.
  function visibleRowCount() {
    return viewMode === 'details'
      ? (detailsViewRef.current?.getVisibleRowCount() ?? 1)
      : (graphicalViewRef.current?.getVisibleRowCount() ?? 1)
  }

  function focusByRowIndex(nextRowIndex: number) {
    const bounded = clamp(nextRowIndex, 0, rowCount - 1)

    if (hasParent && bounded === 0) {
      setFocusedEntry(paneId, PARENT_ROW_ID)
      setSelection(paneId, [], null, null)
      if (viewMode === 'details') detailsViewRef.current?.scrollToRow(0)
      return
    }

    const nextEntry = pane.entries[bounded - parentOffset]
    if (!nextEntry) {
      return
    }

    setFocusedEntry(paneId, nextEntry.id)
    setSelection(paneId, [nextEntry.id], nextEntry.id, nextEntry.id)
    if (viewMode === 'details') detailsViewRef.current?.scrollToRow(bounded)
    else graphicalViewRef.current?.scrollToEntry(bounded - parentOffset)
  }

  function focusPaneShell() {
    setActivePane(paneId)
    paneRef.current?.focus()
  }

  async function submitRename() {
    if (!rename || rename.busy || renameSubmittingRef.current) {
      return
    }

    const trimmed = rename.value.trim()
    if (!trimmed) {
      cancelRename()
      renameSubmittingRef.current = false
      return
    }

    if (trimmed === rename.initialValue) {
      cancelRename()
      focusPaneShell()
      renameSubmittingRef.current = false
      return
    }

    renameSubmittingRef.current = true
    setRenameBusy(true)
    setRenameError(null)
    try {
      await renameEntryInPane(rename.paneId, rename.path, trimmed)
      renameSubmittingRef.current = false
      cancelRename()
      focusPaneShell()
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('inline rename failed', { paneId, path: rename.path, error: message })
      setRenameError(message)
      setRenameBusy(false)
      renameSubmittingRef.current = false
    }
  }

  async function activateEntry(entryId: string) {
    executeCommand('open', paneId, entryId)
  }

  function activateEntryFromPointer(entryId: string, clickCount?: number, eventTimeStamp?: number) {
    if (clickCount !== undefined) {
      const lastActivationAt = lastPointerActivationAtRef.current
      if (
        clickCount > DOUBLE_CLICK_COUNT &&
        lastActivationAt !== null &&
        eventTimeStamp !== undefined &&
        eventTimeStamp - lastActivationAt < macroContinuationWindowMs
      ) {
        return
      }

      if (eventTimeStamp !== undefined) {
        lastPointerActivationAtRef.current = eventTimeStamp
      }
    }
    void activateEntry(entryId)
  }

  // Every completed pair of clicks on ".." goes up one level, regardless of
  // what event.detail the webview reports. Clicks landing right after another
  // pointer activation are strays from that same burst and are dropped.
  const parentPendingClickAtRef = useRef<number | null>(null)
  function activateParentFromPointer(eventTimeStamp?: number) {
    if (eventTimeStamp === undefined) {
      void activateEntry(PARENT_ROW_ID)
      return
    }
    const lastActivationAt = lastPointerActivationAtRef.current
    if (
      lastActivationAt !== null &&
      eventTimeStamp - lastActivationAt < macroContinuationWindowMs
    ) {
      return
    }
    const pendingAt = parentPendingClickAtRef.current
    if (pendingAt !== null && eventTimeStamp - pendingAt <= parentClickPairWindowMs) {
      parentPendingClickAtRef.current = null
      lastPointerActivationAtRef.current = eventTimeStamp
      void activateEntry(PARENT_ROW_ID)
    } else {
      parentPendingClickAtRef.current = eventTimeStamp
    }
  }

  function selectWithModifiers(entryId: string, event: React.MouseEvent<HTMLDivElement>) {
    const currentIds = selection.selectedIds
    const nextIndex = pane.entries.findIndex((entry) => entry.id === entryId)

    if (event.shiftKey && selection.anchorId) {
      const anchorIndex = pane.entries.findIndex((entry) => entry.id === selection.anchorId)
      const start = Math.min(anchorIndex, nextIndex)
      const end = Math.max(anchorIndex, nextIndex)
      const rangeIds = pane.entries.slice(start, end + 1).map((entry) => entry.id)
      setSelection(paneId, rangeIds, selection.anchorId, entryId)
    } else if (event.ctrlKey || event.metaKey) {
      const selectedIds = currentIds.includes(entryId)
        ? currentIds.filter((item) => item !== entryId)
        : [...currentIds, entryId]
      setSelection(paneId, selectedIds, selection.anchorId ?? entryId, entryId)
    } else {
      setSelection(paneId, [entryId], entryId, entryId)
    }

    setFocusedEntry(paneId, entryId)
  }

  function entryIdsInMarquee(rect: MarqueeRect) {
    if (viewMode !== 'details') {
      return graphicalViewRef.current?.getEntriesIntersectingRect(rect) ?? []
    }
    return detailsEntryIdsInRect(rect, pane.entries, {
      listInsetPx: DETAILS_LIST_INSET_PX,
      rowHeightPx: DETAILS_ROW_HEIGHT_PX,
      rowCount,
      parentOffset,
    })
  }

  const { marqueeRect, handleContainerMouseDown } = useMarqueeSelection({
    paneId,
    scrollContainerRef: parentRef,
    selectedIds: selection.selectedIds,
    entryIdsInRect: entryIdsInMarquee,
    onPointerDown: focusPaneShell,
    setSelection,
    setFocusedEntry,
  })

  const {
    dropTargetEntryId,
    isPaneDropTarget,
    clearDragState,
    handleRowDragStart,
    handleFolderDragOver,
    handleFolderDragLeave,
    handleFolderDrop,
    handlePaneDragOver,
    handlePaneDragLeave,
    handlePaneDrop,
  } = usePaneDragSource({
    paneId,
    path: pane.path,
    entries: pane.entries,
    selectedIds: selection.selectedIds,
    platform,
  })

  const bodyState = paneBodyState({
    showSkeleton,
    loading: pane.loading,
    error: pane.error,
    rowCount,
  })
  const newFolderAction = isTrashPath(pane.path)
    ? undefined
    : () => useActionDialogStore.getState().open({ kind: 'newFolder', paneId })

  function showMenu(event: React.MouseEvent, entryId?: string) {
    event.preventDefault()
    // Stop the event from bubbling to the outer scroll-container / section
    // context-menu handlers, which would otherwise re-open `showMenu` without an
    // entry and replace this row's menu with the empty-area menu (hiding the
    // row's Rename/Delete/… actions).
    event.stopPropagation()
    setActivePane(paneId)

    const entry = entryId ? pane.entries.find((item) => item.id === entryId) : undefined
    if (entry && !selection.selectedIds.includes(entry.id)) {
      setSelection(paneId, [entry.id], entry.id, entry.id)
      setFocusedEntry(paneId, entry.id)
    }

    const target = resolveMenuTarget(paneId, entry)
    const { title, chip } = describeMenuTarget(target)
    openMenu({
      paneId,
      title,
      chip,
      x: event.clientX,
      y: event.clientY,
      ...buildContextMenuContent(paneId, target, platform),
      targetId: entryId,
    })
  }

  function renameCancel() {
    ignoreNextRenameBlurRef.current = true
    renameSubmittingRef.current = false
    cancelRename()
    focusPaneShell()
  }

  function renameBlur() {
    if (ignoreNextRenameBlurRef.current) {
      ignoreNextRenameBlurRef.current = false
      return
    }
    void submitRename()
  }

  async function middleClickOpen(entryId: string) {
    const entry = pane.entries.find((item) => item.id === entryId)
    if (!entry) {
      return
    }
    await openTabFromPath(paneId, entry.path)
  }

  // `FileRow` is `React.memo`-wrapped: for that to actually skip re-rendering
  // rows whose own props are unchanged, every row must receive the *same*
  // handler object across renders. Every handler below closes over "current"
  // pane state (entries/selection/rename/...), so it's re-assigned into this
  // ref on every render; the memoized `actions` dispatcher reads through the
  // ref — its identity never changes even though the behaviour it invokes
  // always sees up-to-date state.
  const latestHandlers = {
    focusPaneShell,
    activateEntryFromPointer,
    selectWithModifiers,
    showMenu,
    middleClickOpen,
    handleRowDragStart,
    clearDragState,
    handleFolderDragOver,
    handleFolderDragLeave,
    handleFolderDrop,
    setRenameValue,
    submitRename,
    renameCancel,
    renameBlur,
  }
  const latestRef = useRef(latestHandlers)
  // Refreshed synchronously after every commit (before the browser paints and
  // therefore before any user interaction can reach the dispatcher below) —
  // never written during render itself.
  useLayoutEffect(() => {
    latestRef.current = latestHandlers
  })

  const actions = useMemo<FileRowActions>(
    () => ({
      onPointerDown: () => latestRef.current.focusPaneShell(),
      onActivate: (entryId, clickCount, eventTimeStamp) =>
        latestRef.current.activateEntryFromPointer(entryId, clickCount, eventTimeStamp),
      onClick: (entryId, event) => latestRef.current.selectWithModifiers(entryId, event),
      onContextMenu: (entryId, event) => latestRef.current.showMenu(event, entryId),
      onMiddleClick: (entryId) => void latestRef.current.middleClickOpen(entryId),
      onDragStart: (entryId, event) => latestRef.current.handleRowDragStart(entryId, event),
      onDragEnd: () => latestRef.current.clearDragState(),
      onDragEnter: (entryId, event) => latestRef.current.handleFolderDragOver(entryId, event),
      onDragOver: (entryId, event) => latestRef.current.handleFolderDragOver(entryId, event),
      onDragLeave: (entryId) => latestRef.current.handleFolderDragLeave(entryId),
      onDrop: (entryId, event) => void latestRef.current.handleFolderDrop(entryId, event),
      onRenameChange: (value) => latestRef.current.setRenameValue(value),
      onRenameSubmit: () => void latestRef.current.submitRename(),
      onRenameCancel: () => latestRef.current.renameCancel(),
      onRenameBlur: () => latestRef.current.renameBlur(),
    }),
    [],
  )

  function navigationKeyTable(): KeyHandlerTable {
    const rowNavigation = rowNavigationKeyTable({
      focusedRowIndex,
      rowCount,
      visibleRowCount,
      focusRow: focusByRowIndex,
    })
    if (viewMode === 'details') {
      return rowNavigation
    }
    return {
      ...rowNavigation,
      ...gridMovementKeyTable((movement) => graphicalViewRef.current?.moveFocus(movement)),
    }
  }

  function openFromFilter(entryId: string) {
    focusPaneShell()
    void activateEntry(entryId)
  }

  const handlePaneKeyDown = usePaneKeyboard({
    paneId,
    acceptsKeys: isActivePane && !modalOpen,
    keymap,
    focusedEntryId: pane.focusedEntryId,
    navigationKeyTable,
    openFromFilter,
  })

  const retryListing = () => void reloadPane(paneId)
  const sharedBodyRenderers = {
    permissionDenied: () => <PermissionDenied />,
    error: () => <ErrorState path={pane.path} message={pane.error ?? ''} onRetry={retryListing} />,
    listing: () => undefined,
  }
  const detailsBodyRenderers: PaneBodyRenderers = {
    ...sharedBodyRenderers,
    skeleton: () => (
      <div
        className={`min-h-0 flex-1 overflow-x-auto overflow-y-auto overscroll-contain ${SCROLLBAR_THIN}`}
      >
        <LoadingSkeleton />
      </div>
    ),
    empty: () => (
      <div
        className={`flex min-h-0 flex-1 flex-col ${dropTargetOutline(isPaneDropTarget)}`}
        onDragOver={handlePaneDragOver}
        onDragLeave={handlePaneDragLeave}
        onDrop={(event) => void handlePaneDrop(event)}
      >
        <EmptyState onNewFolder={newFolderAction} />
      </div>
    ),
  }
  const graphicalBodyRenderers: PaneBodyRenderers = {
    ...sharedBodyRenderers,
    skeleton: () => (
      <div className="min-h-0 flex-1 overflow-auto">
        <LoadingSkeleton />
      </div>
    ),
    empty: () => (
      <div className="flex min-h-0 flex-1 flex-col">
        <EmptyState onNewFolder={newFolderAction} />
      </div>
    ),
  }

  return (
    <section
      ref={paneRef}
      data-pane-id={paneId}
      aria-label={pane.title}
      className="relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden border border-light-stroke bg-light-layer outline-none dark:border-dark-stroke dark:bg-dark-layer"
      onMouseDown={() => setActivePane(paneId)}
      onKeyDown={(event) => handlePaneKeyDown(event, false)}
      tabIndex={0}
      onContextMenu={(event) => {
        if (event.target === parentRef.current) {
          showMenu(event)
        }
      }}
    >
      <BreadcrumbBar pane={pane} />
      {viewMode === 'details' ? (
        <DetailsView
          ref={detailsViewRef}
          pane={pane}
          isActivePane={isActivePane}
          hasParent={hasParent}
          selectedIds={selectedIdSet}
          cutEntryPaths={cutEntryPaths}
          dropTargetEntryId={dropTargetEntryId}
          isPaneDropTarget={isPaneDropTarget}
          rename={rename}
          actions={actions}
          usesDetachedMacScrollbars={usesDetachedMacScrollbars}
          marqueeRect={marqueeRect}
          onVisibleRangeChange={handleVisibleRangeChange}
          onScroll={recordScroll}
          onContainerMouseDown={handleContainerMouseDown}
          onContainerContextMenu={(event) => showMenu(event)}
          onPaneDragOver={handlePaneDragOver}
          onPaneDragLeave={handlePaneDragLeave}
          onPaneDrop={(event) => void handlePaneDrop(event)}
          onParentFocus={() => focusByRowIndex(0)}
          onParentActivate={activateParentFromPointer}
          scrollContainerRef={parentRef}
          body={detailsBodyRenderers[bodyState]()}
        />
      ) : (
        <>
          <GraphicalSortBar pane={pane} />
          <GraphicalView
            ref={graphicalViewRef}
            pane={pane}
            mode={viewMode}
            isActivePane={isActivePane}
            selectedIds={selectedIdSet}
            cutEntryPaths={cutEntryPaths}
            dropTargetEntryId={dropTargetEntryId}
            isPaneDropTarget={isPaneDropTarget}
            rename={rename}
            actions={actions}
            marqueeRect={marqueeRect}
            onVisibleRangeChange={handleVisibleRangeChange}
            onThumbnailRangeChange={handleThumbnailRangeChange}
            onFocusEntry={(entryId) => {
              setFocusedEntry(paneId, entryId)
              setSelection(paneId, [entryId], entryId, entryId)
            }}
            onContainerMouseDown={handleContainerMouseDown}
            onScroll={recordScroll}
            onContainerContextMenu={(event) => showMenu(event)}
            onPaneDragOver={handlePaneDragOver}
            onPaneDragLeave={handlePaneDragLeave}
            onPaneDrop={(event) => void handlePaneDrop(event)}
            scrollContainerRef={parentRef}
            body={graphicalBodyRenderers[bodyState]()}
          />
        </>
      )}
    </section>
  )
}
