import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react'
import {
  buildContextMenuContent,
  describeMenuTarget,
  resolveMenuTarget,
} from '@/components/menus/menu-definitions'
import { BreadcrumbBar } from './BreadcrumbBar'
import { FileRow } from './FileRow'
import { HeaderRow } from './HeaderRow'
import { ParentRow } from './ParentRow'
import { TabBar } from './TabBar'
import { executeCommand } from '@/lib/commands'
import { detectPlatformOs, resolveCommandForEvent } from '@/lib/keymap'
import { getParentPath } from '@/stores/panes-store'
import { ConflictDialog } from '@/components/dialogs/ConflictDialog'
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
import { usePanesStore } from '@/stores/panes-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { useSelectionStore } from '@/stores/selection-store'
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

  return settingsOpen || actionDialog !== null || propertiesDialog !== null || contextMenu !== null
}

export function FilePane({ paneId }: FilePaneProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const setActivePane = usePanesStore((state) => state.setActivePane)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  const goUp = usePanesStore((state) => state.goUp)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const requestVisibleRange = usePanesStore((state) => state.setVisibleRange)
  const requestVisibleIcons = usePanesStore((state) => state.requestVisibleIcons)
  const setScrollPosition = usePanesStore((state) => state.setScrollPosition)
  const selection = useSelectionStore((state) => state.selections[paneId])
  const keymap = useKeymapStore((state) => state.bindings)
  const conflict = useQueueStore(activeConflict)
  const resolveConflict = useQueueStore((state) => state.resolve)
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
  const scrollPathRef = useRef(pane.path)
  const lastPointerActivationRef = useRef<{ path: string; activatedAt: number } | null>(null)
  const renameSubmittingRef = useRef(false)
  const ignoreNextRenameBlurRef = useRef(false)
  const detachMarqueeListenersRef = useRef<(() => void) | null>(null)
  const visibleIconRequestTimerRef = useRef<number | undefined>(undefined)
  const [marqueeRect, setMarqueeRect] = useState<MarqueeRect | null>(null)
  const isActivePane = activePaneId === paneId
  const os = detectPlatformOs()
  // Suppress the loading skeleton on fast loads: it only appears once loading
  // has lasted longer than a second, avoiding a jarring flash-and-replace when
  // a folder resolves in a few milliseconds.
  const showSkeleton = useDelayedFlag(pane.loading, 1000)

  const hasParent = getParentPath(pane.path) !== null
  const parentOffset = hasParent ? 1 : 0
  const rowCount = pane.entries.length + parentOffset
  const cutEntryPaths = useMemo(
    () =>
      clipboardMode === 'move'
        ? new Set(clipboardEntries.map((entry) => entry.path.toLowerCase()))
        : new Set<string>(),
    [clipboardEntries, clipboardMode],
  )

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
  const totalHeight = virtualItems.length > 0 ? rowVirtualizer.getTotalSize() : rowCount * rowHeightPx
  const savedScrollTop = pane.scrollPositions[pane.path] ?? 0

  useLayoutEffect(() => {
    const scrollElement = parentRef.current
    if (!scrollElement) {
      return
    }

    const previousPath = scrollPathRef.current
    if (previousPath !== pane.path) {
      setScrollPosition(paneId, previousPath, scrollElement.scrollTop)
      scrollPathRef.current = pane.path
    }

    if (scrollElement.scrollTop !== savedScrollTop) {
      scrollElement.scrollTop = savedScrollTop
    }
  }, [pane.path, paneId, savedScrollTop, setScrollPosition, totalHeight])

  useEffect(() => {
    const items = itemsToRender
    if (items.length === 0) {
      return
    }

    // Report the visible range in entry coordinates (excluding the synthetic
    // parent row) so size requests target real directory entries only.
    const start = Math.max(0, items[0].index - parentOffset)
    const end = Math.max(0, items[items.length - 1].index - parentOffset)
    requestVisibleRange(paneId, start, end)

    // Native icons are lazy and per-visible-row: debounce so fast scrolling
    // doesn't spam the backend with a request per frame.
    window.clearTimeout(visibleIconRequestTimerRef.current)
    visibleIconRequestTimerRef.current = window.setTimeout(() => {
      const visiblePaths = pane.entries.slice(start, end + 1).map((entry) => entry.path)
      void requestVisibleIcons(paneId, visiblePaths)
    }, visibleIconRequestDebounceMs)
  }, [itemsToRender, paneId, pane.entries, parentOffset, requestVisibleIcons, requestVisibleRange])

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

  function selectWithModifiers(entryId: string, event: React.MouseEvent<HTMLButtonElement>) {
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
      x: (clientX - bounds.left) / zoom + container.scrollLeft,
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

  return (
    <section
      ref={paneRef}
      data-pane-id={paneId}
      aria-label={pane.title}
      className={`relative flex h-full min-h-0 flex-col bg-light-surface dark:bg-dark-surface ${
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
      <HeaderRow pane={pane} />
      {showSkeleton ? (
        <div className="min-h-0 flex-1 overflow-auto scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint">
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
        <EmptyState />
      ) : (
        <div
          ref={parentRef}
          data-testid={`file-pane-scroll-${paneId}`}
          className="min-h-0 flex-1 overflow-auto scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint"
          onMouseDown={handleContainerMouseDown}
          onScroll={(event) => setScrollPosition(paneId, pane.path, event.currentTarget.scrollTop)}
          onContextMenu={(event) => showMenu(event)}
        >
          {/*
            Styling-constraint exception: runtime geometry only. The total
            scroll height and each row's translateY come from
            @tanstack/react-virtual (D18) and are continuous px values that no
            static utility/@theme token can express. Every design-system value
            (color/spacing/typography) elsewhere stays a pure Tailwind utility.
          */}
          <div style={{ height: `${totalHeight}px`, position: 'relative' }} className="min-h-full">
            {itemsToRender.map((virtualRow) => {
              const rowStyle = {
                position: 'absolute' as const,
                top: 0,
                left: 0,
                width: '100%',
                transform: `translateY(${virtualRow.start}px)`,
              }

              if (hasParent && virtualRow.index === 0) {
                return (
                  <div key={PARENT_ROW_ID} style={rowStyle}>
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
                <div key={entry.id} style={rowStyle}>
                  <FileRow
                    entry={entry}
                    isActivePane={isActivePane}
                    isFocused={pane.focusedEntryId === entry.id}
                    isSelected={selection.selectedIds.includes(entry.id)}
                    isCut={cutEntryPaths.has(entry.path.toLowerCase())}
                    isRenaming={rename?.entryId === entry.id}
                    renameValue={rename?.entryId === entry.id ? rename.value : ''}
                    renameBusy={rename?.entryId === entry.id ? rename.busy : false}
                    renameError={rename?.entryId === entry.id ? rename.error : null}
                    onPointerDown={focusPaneShell}
                    onActivate={(eventTimeStamp) =>
                      activateEntryFromPointer(entry.id, eventTimeStamp)
                    }
                    onClick={(event) => selectWithModifiers(entry.id, event)}
                    onMiddleClick={() => void openTabFromPath(paneId, entry.path)}
                    onContextMenu={(event) => showMenu(event, entry.id)}
                    onRenameChange={(value) => setRenameValue(value)}
                    onRenameSubmit={() => void submitRename()}
                    onRenameCancel={() => {
                      ignoreNextRenameBlurRef.current = true
                      renameSubmittingRef.current = false
                      cancelRename()
                      focusPaneShell()
                    }}
                    onRenameBlur={() => {
                      if (ignoreNextRenameBlurRef.current) {
                        ignoreNextRenameBlurRef.current = false
                        return
                      }
                      void submitRename()
                    }}
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
      )}
      {isActivePane && conflict ? (
        <ConflictDialog
          key={conflict.operationId}
          conflict={conflict}
          onResolve={(resolution, applyToAll, renameTo) =>
            resolveConflict(conflict.operationId, resolution, applyToAll, renameTo)
          }
        />
      ) : null}
    </section>
  )
}
