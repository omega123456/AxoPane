import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import {
  buildContextMenuContent,
  describeMenuTarget,
  resolveMenuTarget,
} from '@/components/menus/menu-definitions'
import { BreadcrumbBar } from './BreadcrumbBar'
import { FileRow, type FileRowActions } from './FileRow'
import { HeaderRow, paneContentWidth } from './HeaderRow'
import { ParentRow } from './ParentRow'
import { TabBar } from './TabBar'
import { executeCommand } from '@/lib/commands'
import { detectPlatformOs, resolveCommandForEvent } from '@/lib/keymap'
import { getParentPath } from '@/stores/panes-store'
import { EmptyState } from '@/components/states/EmptyState'
import { ErrorState } from '@/components/states/ErrorState'
import { EverythingBanner } from '@/components/states/EverythingBanner'
import { LoadingSkeleton } from '@/components/states/LoadingSkeleton'
import { PermissionDenied } from '@/components/states/PermissionDenied'
import { useDelayedFlag } from '@/lib/use-delayed-flag'
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { renameEntryInPane } from '@/lib/file-actions'
import { log } from '@/lib/app-log-commands'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useDragStore } from '@/stores/drag-store'
import { useNativeMenuWarmStore } from '@/stores/native-menu-warm-store'
import { canDropInto, performDrop, resolveDropKind, type DragItem } from '@/lib/drag-drop'
import type { PaneId } from '@/types/pane'

function isPermissionError(message: string | null) {
  if (!message) {
    return false
  }

  return /permission|denied|forbidden|access is denied/i.test(message)
}

const PARENT_ROW_ID = '..'
const pointerNavigationCooldownMs = 400
const rowHeightPx = 30
const marqueeDragThresholdPx = 4
const visibleIconRequestDebounceMs = 100

type MarqueeRect = { left: number; top: number; width: number; height: number }
type MarqueeDragState = {
  startX: number
  startY: number
  additive: boolean
  baseSelectedIds: string[]
}

function isEditableTarget(target: EventTarget | null) {
  return (
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    (target instanceof HTMLElement && target.isContentEditable)
  )
}

type FilePaneProps = {
  paneId: PaneId
}

function useModalOpen() {
  const settingsOpen = useSettingsStore((state) => state.isOpen)
  const actionDialog = useActionDialogStore((state) => state.dialog)
  const propertiesDialog = usePropertiesDialogStore((state) => state.dialog)
  const contextMenu = useContextMenuStore((state) => state.menu)
  const conflict = useQueueStore(activeConflict)

  return (
    settingsOpen ||
    actionDialog !== null ||
    propertiesDialog !== null ||
    contextMenu !== null ||
    conflict !== undefined
  )
}

export function FilePane({ paneId }: FilePaneProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const focusRequestId = usePanesStore((state) => state.focusRequestId)
  const focusRequestPaneId = usePanesStore((state) => state.focusRequestPaneId)
  const setActivePane = usePanesStore((state) => state.setActivePane)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  const goUp = usePanesStore((state) => state.goUp)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const requestVisibleIcons = usePanesStore((state) => state.requestVisibleIcons)
  const warmVisibleNativeMenus = useNativeMenuWarmStore((state) => state.warmVisibleNativeMenus)
  const setScrollPosition = usePanesStore((state) => state.setScrollPosition)
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columnWidths = useLayoutStore((state) => state.columnWidths)
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
  const headerScrollRef = useRef<HTMLDivElement | null>(null)
  const parentRef = useRef<HTMLDivElement | null>(null)
  const horizontalScrollRef = useRef<HTMLDivElement | null>(null)
  const contentLayerRef = useRef<HTMLDivElement | null>(null)
  const horizontalScrollLeftRef = useRef(0)
  const scrollPathRef = useRef(pane.path)
  // Tracks the live scrollTop for the currently-mounted path without writing
  // to the store on every scroll event (that would re-render the whole app on
  // every frame of scrolling). Kept in sync with the DOM on every scroll and
  // whenever the path-change effect below restores/persists a position.
  const liveScrollTopRef = useRef(0)
  // Target scrollTop the layout effect below is still trying to apply after a
  // path switch (kept while retrying across a couple of re-runs while
  // `totalHeight` grows enough for the target to actually stick — see the
  // effect for details). `null` once the restore has taken effect (or there is
  // no path switch in flight), so a same-path re-run triggered by something
  // else entirely (e.g. a fs-watch patch changing the entry count) never
  // clobbers the user's current live scroll position.
  const pendingRestoreRef = useRef<number | null>(pane.scrollPositions[pane.path] ?? 0)
  // True for exactly the first evaluation of a given `pendingRestoreRef` arm
  // (right after mount or a path switch). While true, the layout effect below
  // gives the restore the benefit of the doubt even if it looks unreachable
  // this render — `totalHeight` may still be about to grow once async content
  // (e.g. entries arriving after the initial render) lands. From the second
  // evaluation onward, a restore that still can't reach its target is treated
  // as permanently unreachable (see the effect for the exact conditions).
  const restoreJustArmedRef = useRef(true)
  const lastPointerActivationRef = useRef<{ path: string; activatedAt: number } | null>(null)
  const renameSubmittingRef = useRef(false)
  const ignoreNextRenameBlurRef = useRef(false)
  const detachMarqueeListenersRef = useRef<(() => void) | null>(null)
  const visibleIconRequestTimerRef = useRef<number | undefined>(undefined)
  const [marqueeRect, setMarqueeRect] = useState<MarqueeRect | null>(null)
  const beginDrag = useDragStore((state) => state.begin)
  const endDrag = useDragStore((state) => state.end)
  // Highlight state for internal drag-and-drop: the directory row currently
  // hovered by a valid drag, and whether the pane background (its own folder) is.
  const [dropTargetEntryId, setDropTargetEntryId] = useState<string | null>(null)
  const [isPaneDropTarget, setIsPaneDropTarget] = useState(false)
  const isActivePane = activePaneId === paneId
  const os = detectPlatformOs()
  const usesDetachedMacScrollbars = os === 'macos'
  const rowLayerClassName = 'absolute inset-x-0'
  // Suppress the loading skeleton on fast loads: it only appears once loading
  // has lasted longer than a second, avoiding a jarring flash-and-replace when
  // a folder resolves in a few milliseconds.
  const showSkeleton = useDelayedFlag(pane.loading, 1000)

  const hasParent = getParentPath(pane.path) !== null
  const parentOffset = hasParent ? 1 : 0
  const rowCount = pane.entries.length + parentOffset
  const visibleColumns = useMemo(
    () => configuredColumns.filter((column) => column.visible),
    [configuredColumns],
  )
  const contentWidth = useMemo(
    () => paneContentWidth(visibleColumns, columnWidths),
    [columnWidths, visibleColumns],
  )
  const cutEntryPaths = useMemo(
    () =>
      clipboardMode === 'move'
        ? new Set(clipboardEntries.map((entry) => entry.path.toLowerCase()))
        : new Set<string>(),
    [clipboardEntries, clipboardMode],
  )
  // O(1) per-row membership check instead of `selectedIds.includes(...)`
  // (O(selection size)) on every row on every render.
  const selectedIdSet = useMemo(() => new Set(selection.selectedIds), [selection.selectedIds])

  const rowVirtualizer = useElementVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => rowHeightPx,
    overscan: 10,
  })
  const virtualItems = rowVirtualizer.getVirtualItems()
  const itemsToRender =
    virtualItems.length > 0
      ? virtualItems
      : Array.from({ length: rowCount }, (_, index) => ({
          key: `row-${index}`,
          index,
          start: index * rowHeightPx,
        }))
  const totalHeight =
    virtualItems.length > 0 ? rowVirtualizer.getTotalSize() : rowCount * rowHeightPx
  const savedScrollTop = pane.scrollPositions[pane.path] ?? 0

  useEffect(() => {
    if (focusRequestId > 0 && focusRequestPaneId === paneId) {
      paneRef.current?.focus()
    }
  }, [focusRequestId, focusRequestPaneId, paneId])

  useLayoutEffect(() => {
    const scrollElement = parentRef.current
    if (!scrollElement) {
      return
    }

    const previousPath = scrollPathRef.current
    if (previousPath !== pane.path) {
      setScrollPosition(paneId, previousPath, liveScrollTopRef.current)
      scrollPathRef.current = pane.path
      // A path switch always (re)starts a restore attempt targeting the new
      // path's saved position, superseding anything left pending from before.
      pendingRestoreRef.current = savedScrollTop
      restoreJustArmedRef.current = true
    }

    // Only force `scrollTop` while a restore is actually in flight (right
    // after a path switch, possibly across a few re-runs while `totalHeight`
    // grows tall enough for the target to stick). A same-path re-run with no
    // pending restore — e.g. the entry count changing because of a fs-watch
    // patch — must leave the user's live scroll position alone.
    if (pendingRestoreRef.current !== null) {
      const target = pendingRestoreRef.current
      const wasJustArmed = restoreJustArmedRef.current
      if (scrollElement.scrollTop !== target) {
        scrollElement.scrollTop = target
      }
      liveScrollTopRef.current = scrollElement.scrollTop
      // Consider the restore attempt "done" — and stop forcing scrollTop on
      // future same-path re-runs — once no further retry could possibly make
      // progress toward `target`:
      //   - reached (or overshot) the target; or
      //   - within a sub-pixel tolerance of it (a WebView can round a
      //     fractional saved target — this app reads `documentElement.zoom`
      //     for scaling — to an integer that never equals `target` exactly).
      // Beyond the very first evaluation of this arm, also treat the restore
      // as done when it's clamped short with no room left to grow — the
      // folder shrank while the user was away, so `scrollHeight` no longer
      // exceeds `clientHeight` (or the assignment above was silently clamped
      // to something below `target`). The first evaluation is exempted so a
      // fresh path switch still gets one full retry cycle while `totalHeight`
      // catches up with async content (e.g. entries arriving after mount).
      const reachedOrClose =
        scrollElement.scrollTop >= target || Math.abs(scrollElement.scrollTop - target) < 1
      const clampedWithNoRoomToGrow =
        !wasJustArmed &&
        (scrollElement.scrollTop < target ||
          scrollElement.scrollHeight <= scrollElement.clientHeight)
      if (reachedOrClose || clampedWithNoRoomToGrow) {
        pendingRestoreRef.current = null
      }
      restoreJustArmedRef.current = false
    }
  }, [pane.path, paneId, savedScrollTop, setScrollPosition, totalHeight])

  useEffect(() => {
    const items = itemsToRender
    if (items.length === 0) {
      return
    }

    // Visible range in entry coordinates (excluding the synthetic parent row),
    // used only to target lazy icon/native-menu requests at the real directory
    // entries currently on screen.
    const start = Math.max(0, items[0].index - parentOffset)
    const end = Math.max(0, items[items.length - 1].index - parentOffset)

    // Native icons are lazy and per-visible-row: debounce so fast scrolling
    // doesn't spam the backend with a request per frame.
    window.clearTimeout(visibleIconRequestTimerRef.current)
    visibleIconRequestTimerRef.current = window.setTimeout(() => {
      const visiblePaths = pane.entries.slice(start, end + 1).map((entry) => entry.path)
      void requestVisibleIcons(paneId, visiblePaths)
      // Background native-context-menu cache pre-warming reuses this same
      // debounced visible-range trigger and the same visible paths. Unlike the
      // icon request above, warming is not pre-filtered to non-directories:
      // the store derives a File or Folder warm key per entry internally.
      void warmVisibleNativeMenus(paneId, visiblePaths)
    }, visibleIconRequestDebounceMs)
  }, [
    itemsToRender,
    paneId,
    pane.entries,
    parentOffset,
    requestVisibleIcons,
    warmVisibleNativeMenus,
  ])

  useEffect(() => () => window.clearTimeout(visibleIconRequestTimerRef.current), [])

  // Row indices include the synthetic parent row at position 0 when present.
  // Row 0 (with a parent) is the parent row; entry `i` is at row `i + parentOffset`.
  const focusedRowIndex = useMemo(() => {
    if (hasParent && pane.focusedEntryId === PARENT_ROW_ID) {
      return 0
    }
    const entryIndex = pane.entries.findIndex((entry) => entry.id === pane.focusedEntryId)
    return entryIndex < 0 ? -1 : entryIndex + parentOffset
  }, [hasParent, parentOffset, pane.entries, pane.focusedEntryId])

  // Rows fully visible in the scroll viewport, used as the Page Up/Down step —
  // matches Explorer's "jump by a screenful" behavior instead of a fixed count.
  function visibleRowCount() {
    const viewportHeight = parentRef.current?.clientHeight ?? 0
    return Math.max(1, Math.floor(viewportHeight / rowHeightPx))
  }

  function focusByRowIndex(nextRowIndex: number) {
    const bounded = Math.max(0, Math.min(nextRowIndex, rowCount - 1))

    if (hasParent && bounded === 0) {
      setFocusedEntry(paneId, PARENT_ROW_ID)
      setSelection(paneId, [], null, null)
      rowVirtualizer.scrollToIndex(0)
      return
    }

    const nextEntry = pane.entries[bounded - parentOffset]
    if (!nextEntry) {
      return
    }

    setFocusedEntry(paneId, nextEntry.id)
    setSelection(paneId, [nextEntry.id], nextEntry.id, nextEntry.id)
    rowVirtualizer.scrollToIndex(bounded)
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

  function activateEntryFromPointer(entryId: string, eventTimeStamp?: number) {
    const lastActivation = lastPointerActivationRef.current
    const activationTime = eventTimeStamp ?? Number.POSITIVE_INFINITY

    // Mouse-macro double-clicks can keep firing after the pane has already
    // navigated and re-rendered a different row under the pointer. Ignore one
    // immediate follow-up activation across a path change so the gesture opens
    // exactly one level.
    if (
      lastActivation &&
      lastActivation.path !== pane.path &&
      activationTime - lastActivation.activatedAt < pointerNavigationCooldownMs
    ) {
      return
    }

    lastPointerActivationRef.current = { path: pane.path, activatedAt: activationTime }
    void activateEntry(entryId)
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

  // Windows-Explorer-style rubber-band selection. A mousedown on the empty
  // background of the row list (not on a row button) starts tracking; rows
  // whose row-span vertically overlaps the drawn rectangle become selected.
  // Because every row is a fixed `rowHeightPx` tall and rendered in
  // `pane.entries` order, the intersecting rows are always a contiguous
  // index range, so no per-row hit-testing loop is needed.
  function contentPoint(clientX: number, clientY: number) {
    const container = parentRef.current
    if (!container) {
      return null
    }
    const bounds = container.getBoundingClientRect()
    const zoom = Number.parseFloat(getComputedStyle(document.documentElement).zoom) || 1
    return {
      x:
        (clientX - bounds.left) / zoom +
        (usesDetachedMacScrollbars ? horizontalScrollLeftRef.current : container.scrollLeft),
      y: (clientY - bounds.top) / zoom + container.scrollTop,
    }
  }

  function applyMarqueeSelection(drag: MarqueeDragState, rectTop: number, rectBottom: number) {
    const firstRow = Math.max(0, Math.floor(rectTop / rowHeightPx))
    const lastRow = Math.min(rowCount - 1, Math.ceil(rectBottom / rowHeightPx) - 1)
    const entryStart = Math.max(0, firstRow - parentOffset)
    const entryEnd = lastRow - parentOffset
    const rangeIds =
      lastRow >= firstRow && entryEnd >= entryStart
        ? pane.entries.slice(entryStart, entryEnd + 1).map((entry) => entry.id)
        : []

    const selectedIds = drag.additive
      ? Array.from(new Set([...drag.baseSelectedIds, ...rangeIds]))
      : rangeIds
    const anchorId = selectedIds[0] ?? null
    const focusedId = rangeIds[rangeIds.length - 1] ?? selectedIds[selectedIds.length - 1] ?? null

    setSelection(paneId, selectedIds, anchorId, focusedId)
    if (focusedId) {
      setFocusedEntry(paneId, focusedId)
    }
  }

  function handleContainerMouseDown(event: React.MouseEvent<HTMLDivElement>) {
    focusPaneShell()

    if (event.button !== 0) {
      return
    }
    if (event.target instanceof Element && event.target.closest('[role="row"]')) {
      return
    }

    const point = contentPoint(event.clientX, event.clientY)
    if (!point) {
      return
    }

    if (!(event.ctrlKey || event.metaKey)) {
      setSelection(paneId, [], null, null)
    }

    const drag: MarqueeDragState = {
      startX: point.x,
      startY: point.y,
      additive: event.ctrlKey || event.metaKey,
      baseSelectedIds: selection.selectedIds,
    }

    // Closures are created fresh per drag and captured by reference below, so
    // the same function identities are used for both add and remove — safe
    // even across the re-renders that `applyMarqueeSelection` triggers.
    function onMouseMove(moveEvent: globalThis.MouseEvent) {
      const point = contentPoint(moveEvent.clientX, moveEvent.clientY)
      if (!point) {
        return
      }

      const left = Math.min(drag.startX, point.x)
      const top = Math.min(drag.startY, point.y)
      const width = Math.abs(point.x - drag.startX)
      const height = Math.abs(point.y - drag.startY)

      if (width < marqueeDragThresholdPx && height < marqueeDragThresholdPx) {
        setMarqueeRect(null)
        return
      }

      setMarqueeRect({ left, top, width, height })
      applyMarqueeSelection(drag, top, top + height)
    }

    function onMouseUp() {
      setMarqueeRect(null)
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
      detachMarqueeListenersRef.current = null
    }

    document.addEventListener('mousemove', onMouseMove)
    document.addEventListener('mouseup', onMouseUp)
    detachMarqueeListenersRef.current = () => {
      document.removeEventListener('mousemove', onMouseMove)
      document.removeEventListener('mouseup', onMouseUp)
    }
  }

  useEffect(() => () => detachMarqueeListenersRef.current?.(), [])

  const canGoUp = hasParent
  const permissionDenied = isPermissionError(pane.error)
  // An empty folder still shows the synthetic ".." row when a parent exists, so
  // the dedicated EmptyState only takes over when there is genuinely nothing to
  // render (empty folder at a filesystem root).
  const showEmpty = !pane.loading && !pane.error && rowCount === 0

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
      ...buildContextMenuContent(paneId, target, os),
      targetId: entryId,
    })
  }

  // Internal drag-and-drop. Copy-vs-move follows platform convention: on Windows
  // Ctrl forces copy and Shift forces move; on macOS Option forces copy and
  // Cmd forces move. `resolveDropKind` reads these as generic force-copy /
  // force-move flags so its logic stays OS-agnostic.
  function dropModifiers(event: React.DragEvent) {
    if (os === 'macos') {
      return { ctrlKey: event.altKey, shiftKey: event.metaKey }
    }
    return { ctrlKey: event.ctrlKey, shiftKey: event.shiftKey }
  }

  function handleRowDragStart(entryId: string, event: React.DragEvent<HTMLDivElement>) {
    const entry = pane.entries.find((item) => item.id === entryId)
    // The synthetic parent row and trash rows have no real transferable source.
    if (!entry || entry.trashId) {
      event.preventDefault()
      return
    }
    // Drag the whole selection when the grabbed row belongs to it; otherwise
    // just the grabbed row.
    const source = selection.selectedIds.includes(entry.id)
      ? pane.entries.filter((item) => selection.selectedIds.includes(item.id))
      : [entry]
    const items: DragItem[] = source
      .filter((item) => !item.trashId)
      .map((item) => ({
        id: item.id,
        name: item.name,
        path: item.path,
        isDir: item.isDir,
        sizeBytes: item.sizeBytes,
      }))
    if (items.length === 0) {
      event.preventDefault()
      return
    }
    beginDrag({ sourcePaneId: paneId, sourceDir: pane.path, items })
    event.dataTransfer.effectAllowed = 'copyMove'
    event.dataTransfer.setData('text/plain', items.map((item) => item.path).join('\n'))
  }

  function clearDragState() {
    endDrag()
    setDropTargetEntryId(null)
    setIsPaneDropTarget(false)
  }

  function handleFolderDragOver(entryId: string, event: React.DragEvent<HTMLDivElement>) {
    const entry = pane.entries.find((item) => item.id === entryId)
    // Non-directory rows are never a valid drop target (mirrors the previous
    // behaviour of only wiring this handler up for directory rows at all).
    if (!entry || !entry.isDir) {
      return
    }
    const drag = useDragStore.getState().drag
    if (!canDropInto(drag, entry.path, os)) {
      return
    }
    // preventDefault marks this a valid drop target; stopPropagation keeps the
    // pane-background highlight from also lighting up while over a folder row.
    event.preventDefault()
    event.stopPropagation()
    event.dataTransfer.dropEffect = resolveDropKind(
      dropModifiers(event),
      drag!.sourceDir,
      entry.path,
      os,
    )
    setDropTargetEntryId(entry.id)
  }

  function handleFolderDragLeave(entryId: string) {
    setDropTargetEntryId((current) => (current === entryId ? null : current))
  }

  async function handleFolderDrop(entryId: string, event: React.DragEvent<HTMLDivElement>) {
    const entry = pane.entries.find((item) => item.id === entryId)
    if (!entry || !entry.isDir) {
      return
    }
    event.preventDefault()
    event.stopPropagation()
    const drag = useDragStore.getState().drag
    const modifiers = dropModifiers(event)
    clearDragState()
    await performDrop(drag, entry.path, modifiers, os)
  }

  function handlePaneDragOver(event: React.DragEvent<HTMLDivElement>) {
    const drag = useDragStore.getState().drag
    if (!canDropInto(drag, pane.path, os)) {
      return
    }
    event.preventDefault()
    event.dataTransfer.dropEffect = resolveDropKind(
      dropModifiers(event),
      drag!.sourceDir,
      pane.path,
      os,
    )
    setIsPaneDropTarget(true)
  }

  function handlePaneDragLeave(event: React.DragEvent<HTMLDivElement>) {
    // Ignore leave events fired when moving onto a child row still inside the pane.
    if (event.currentTarget.contains(event.relatedTarget as Node | null)) {
      return
    }
    setIsPaneDropTarget(false)
  }

  async function handlePaneDrop(event: React.DragEvent<HTMLDivElement>) {
    event.preventDefault()
    const drag = useDragStore.getState().drag
    const modifiers = dropModifiers(event)
    clearDragState()
    await performDrop(drag, pane.path, modifiers, os)
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
      onActivate: (entryId, eventTimeStamp) =>
        latestRef.current.activateEntryFromPointer(entryId, eventTimeStamp),
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

  const syncHorizontalScroll = useCallback((scrollLeft: number) => {
    horizontalScrollLeftRef.current = scrollLeft
    if (headerScrollRef.current) {
      headerScrollRef.current.scrollLeft = scrollLeft
    }
    if (contentLayerRef.current) {
      contentLayerRef.current.style.transform =
        scrollLeft === 0 ? '' : `translateX(-${scrollLeft}px)`
    }
  }, [])

  useLayoutEffect(() => {
    if (!usesDetachedMacScrollbars) {
      horizontalScrollLeftRef.current = 0
      if (contentLayerRef.current) {
        contentLayerRef.current.style.transform = ''
      }
      return
    }

    const scrollLeft = horizontalScrollRef.current?.scrollLeft ?? 0
    syncHorizontalScroll(scrollLeft)
  }, [contentWidth, syncHorizontalScroll, usesDetachedMacScrollbars])

  function handleHorizontalScroll(event: React.UIEvent<HTMLDivElement>) {
    syncHorizontalScroll(event.currentTarget.scrollLeft)
  }

  function handleBodyWheel(event: React.WheelEvent<HTMLDivElement>) {
    if (!usesDetachedMacScrollbars || event.deltaX === 0) {
      return
    }

    const horizontalScroll = horizontalScrollRef.current
    if (!horizontalScroll) {
      return
    }

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

  return (
    <section
      ref={paneRef}
      data-pane-id={paneId}
      aria-label={pane.title}
      className={`relative flex h-full min-h-0 min-w-0 flex-col overflow-hidden bg-light-surface dark:bg-dark-surface ${
        isActivePane ? 'outline outline-1 outline-accent-blue-border' : ''
      }`}
      onMouseDown={() => setActivePane(paneId)}
      onKeyDown={(event) => {
        if (!isActivePane) {
          return
        }

        if (modalOpen) {
          return
        }

        // The filter input is the one editable target inside the pane that still
        // wants list navigation: pressing Up/Down/Enter should drive the filtered
        // list (and open the focused entry) while keeping the cursor in the input
        // so the user can keep refining the filter. Every other editable target
        // (e.g. an inline rename) keeps swallowing keys as before.
        const targetIsFilter =
          event.target instanceof HTMLInputElement &&
          event.target.getAttribute('aria-label') === `${pane.title} filter`

        if (isEditableTarget(event.target) && !targetIsFilter) {
          return
        }

        if (event.key === 'ArrowDown') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex + 1)
        } else if (event.key === 'ArrowUp') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex - 1)
        } else if (event.key === 'PageDown') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex + visibleRowCount())
        } else if (event.key === 'PageUp') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex - visibleRowCount())
        } else if (event.key === 'Home') {
          event.preventDefault()
          focusByRowIndex(0)
        } else if (event.key === 'End') {
          event.preventDefault()
          focusByRowIndex(rowCount - 1)
        } else if (targetIsFilter) {
          // Open the focused entry on Enter; let every other key reach the input
          // so typing into the filter is unaffected.
          if (event.key === 'Enter' && pane.focusedEntryId) {
            event.preventDefault()
            // Hand focus back to the pane shell so navigating into the new folder
            // resumes arrow-key control instead of leaving the cursor trapped in
            // the filter input.
            focusPaneShell()
            void activateEntry(pane.focusedEntryId)
          }
        } else if (
          event.key.length === 1 &&
          // Space is a bound command (Calculate size), not a typeahead-to-filter
          // character — let it fall through to the command resolver below.
          event.key !== ' ' &&
          !event.ctrlKey &&
          !event.metaKey &&
          !event.altKey
        ) {
          const target = document.querySelector<HTMLInputElement>(
            `input[aria-label="${pane.title} filter"]`,
          )
          target?.focus()
          target?.setSelectionRange(target.value.length, target.value.length)
        } else {
          const commandId = resolveCommandForEvent(event.nativeEvent, keymap)
          if (commandId) {
            event.preventDefault()
            // The focused pane owns this command. Stop the event here so the
            // global fallback handler on `window` (App.tsx) doesn't dispatch it a
            // second time — a double paste would otherwise enqueue two transfers.
            event.stopPropagation()
            executeCommand(commandId, paneId)
          }
        }
      }}
      tabIndex={0}
      onContextMenu={(event) => {
        if (event.target === parentRef.current) {
          showMenu(event)
        }
      }}
    >
      <TabBar paneId={paneId} title={pane.title} currentPath={pane.path} isActive={isActivePane} />
      <EverythingBanner />
      <BreadcrumbBar pane={pane} isActive={isActivePane} />
      <div
        ref={headerScrollRef}
        data-testid={`file-pane-header-scroll-${paneId}`}
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
      {showSkeleton ? (
        <div className="min-h-0 flex-1 overflow-x-auto overflow-y-auto overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint">
          <LoadingSkeleton />
        </div>
      ) : permissionDenied ? (
        <PermissionDenied onGoUp={() => void goUp(paneId)} canGoUp={canGoUp} />
      ) : pane.error ? (
        <ErrorState
          message={pane.error}
          onRetry={() => void reloadPane(paneId)}
          onGoUp={() => void goUp(paneId)}
          canGoUp={canGoUp}
        />
      ) : showEmpty ? (
        <div
          className={`min-h-0 flex-1 ${
            isPaneDropTarget ? 'outline outline-2 -outline-offset-2 outline-accent-blue-border' : ''
          }`}
          onDragOver={handlePaneDragOver}
          onDragLeave={handlePaneDragLeave}
          onDrop={(event) => void handlePaneDrop(event)}
        >
          <EmptyState />
        </div>
      ) : (
        <div className="relative min-h-0 flex-1">
          <div
            ref={parentRef}
            data-testid={`file-pane-scroll-${paneId}`}
            className={`h-full min-h-0 overscroll-contain scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint ${
              usesDetachedMacScrollbars
                ? 'overflow-x-hidden overflow-y-auto pb-2'
                : 'overflow-x-auto overflow-y-auto'
            } ${
              isPaneDropTarget
                ? 'outline outline-2 -outline-offset-2 outline-accent-blue-border'
                : ''
            }`}
            onMouseDown={handleContainerMouseDown}
            onWheel={handleBodyWheel}
            onScroll={(event) => {
              liveScrollTopRef.current = event.currentTarget.scrollTop
              if (!usesDetachedMacScrollbars && headerScrollRef.current) {
                headerScrollRef.current.scrollLeft = event.currentTarget.scrollLeft
              }
            }}
            onContextMenu={(event) => showMenu(event)}
            onDragOver={handlePaneDragOver}
            onDragLeave={handlePaneDragLeave}
            onDrop={(event) => void handlePaneDrop(event)}
          >
            {/*
            Styling-constraint exception: runtime geometry only. The total
            scroll height and each row's top offset come from
            @tanstack/react-virtual (D18) and are continuous px values that no
            static utility/@theme token can express. Every design-system value
            (color/spacing/typography) elsewhere stays a pure Tailwind utility.
          */}
            <div
              ref={contentLayerRef}
              style={{
                height: `${totalHeight}px`,
                minWidth: `${contentWidth}px`,
                position: 'relative',
              }}
              className={`min-h-full w-full ${
                usesDetachedMacScrollbars ? 'will-change-transform' : ''
              }`}
            >
              {itemsToRender.map((virtualRow) => {
                const rowStyle = {
                  top: `${virtualRow.start}px`,
                }

                if (hasParent && virtualRow.index === 0) {
                  return (
                    <div key={PARENT_ROW_ID} className={rowLayerClassName} style={rowStyle}>
                      <ParentRow
                        isActivePane={isActivePane}
                        isFocused={pane.focusedEntryId === PARENT_ROW_ID}
                        onPointerDown={focusPaneShell}
                        onActivate={(eventTimeStamp) =>
                          activateEntryFromPointer(PARENT_ROW_ID, eventTimeStamp)
                        }
                        onFocus={() => focusByRowIndex(0)}
                      />
                    </div>
                  )
                }

                const entry = pane.entries[virtualRow.index - parentOffset]
                if (!entry) {
                  return null
                }

                return (
                  <div key={entry.id} className={rowLayerClassName} style={rowStyle}>
                    <FileRow
                      entry={entry}
                      isActivePane={isActivePane}
                      isFocused={pane.focusedEntryId === entry.id}
                      isSelected={selectedIdSet.has(entry.id)}
                      isCut={cutEntryPaths.has(entry.path.toLowerCase())}
                      isDropTarget={dropTargetEntryId === entry.id}
                      draggable={!entry.trashId}
                      isRenaming={rename?.entryId === entry.id}
                      renameValue={rename?.entryId === entry.id ? rename.value : ''}
                      renameBusy={rename?.entryId === entry.id ? rename.busy : false}
                      renameError={rename?.entryId === entry.id ? rename.error : null}
                      actions={actions}
                    />
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
          {usesDetachedMacScrollbars ? (
            <div
              ref={horizontalScrollRef}
              data-testid={`file-pane-horizontal-scroll-${paneId}`}
              className="absolute inset-x-0 bottom-0 z-20 h-2 overflow-x-auto overflow-y-hidden overscroll-contain bg-light-surface scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:bg-dark-surface dark:scrollbar-thumb-dark-text-faint"
              onScroll={handleHorizontalScroll}
            >
              <div style={{ width: `${contentWidth}px`, height: '1px' }} />
            </div>
          ) : null}
        </div>
      )}
    </section>
  )
}
