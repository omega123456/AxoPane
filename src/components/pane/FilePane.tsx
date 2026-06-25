import { useEffect, useMemo, useRef } from 'react'
import {
  buildContextMenuItems,
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
import { useElementVirtualizer } from '@/lib/use-element-virtualizer'
import { usePanesStore } from '@/stores/panes-store'
import { activeConflict, useQueueStore } from '@/stores/queue-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { PaneId } from '@/types/pane'

function isPermissionError(message: string | null) {
  if (!message) {
    return false
  }

  return /permission|denied|forbidden|access is denied/i.test(message)
}

const PARENT_ROW_ID = '..'

type FilePaneProps = {
  paneId: PaneId
}

export function FilePane({ paneId }: FilePaneProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const setActivePane = usePanesStore((state) => state.setActivePane)
  const setFocusedEntry = usePanesStore((state) => state.setFocusedEntry)
  const navigatePane = usePanesStore((state) => state.navigatePane)
  const goUp = usePanesStore((state) => state.goUp)
  const openTabFromPath = usePanesStore((state) => state.openTabFromPath)
  const reloadPane = usePanesStore((state) => state.reloadPane)
  const requestVisibleRange = usePanesStore((state) => state.setVisibleRange)
  const selection = useSelectionStore((state) => state.selections[paneId])
  const keymap = useKeymapStore((state) => state.bindings)
  const conflict = useQueueStore(activeConflict)
  const resolveConflict = useQueueStore((state) => state.resolve)
  const setSelection = useSelectionStore((state) => state.setSelection)
  const openMenu = useContextMenuStore((state) => state.openMenu)
  const parentRef = useRef<HTMLDivElement | null>(null)
  const isActivePane = activePaneId === paneId
  const os = detectPlatformOs()

  const hasParent = getParentPath(pane.path) !== null
  const parentOffset = hasParent ? 1 : 0
  const rowCount = pane.entries.length + parentOffset

  const rowVirtualizer = useElementVirtualizer({
    count: rowCount,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 30,
    overscan: 10,
  })
  const virtualItems = rowVirtualizer.getVirtualItems()
  const itemsToRender =
    virtualItems.length > 0
      ? virtualItems
      : Array.from({ length: rowCount }, (_, index) => ({
          key: `row-${index}`,
          index,
          start: index * 30,
        }))
  const totalHeight =
    virtualItems.length > 0 ? rowVirtualizer.getTotalSize() : rowCount * 30

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
  }, [itemsToRender, paneId, parentOffset, requestVisibleRange])

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

  async function activateEntry(entryId: string) {
    if (entryId === PARENT_ROW_ID) {
      await goUp(paneId)
      return
    }

    const entry = pane.entries.find((item) => item.id === entryId)
    if (!entry) {
      return
    }

    if (entry.isDir) {
      await navigatePane(paneId, entry.path)
    }
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
      items: buildContextMenuItems(paneId, target, os),
      targetId: entryId,
    })
  }

  return (
    <section
      aria-label={pane.title}
      className={`relative flex min-h-0 flex-col bg-light-surface dark:bg-dark-surface ${
        isActivePane ? 'outline outline-1 outline-accent-blue-border' : ''
      }`}
      onMouseDown={() => setActivePane(paneId)}
      onKeyDown={(event) => {
        if (!isActivePane) {
          return
        }

        if (event.key === 'ArrowDown') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex + 1)
        } else if (event.key === 'ArrowUp') {
          event.preventDefault()
          focusByRowIndex(focusedRowIndex - 1)
        } else if (event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
          const target = document.querySelector<HTMLInputElement>(`input[aria-label="${pane.title} filter"]`)
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
      <TabBar
        paneId={paneId}
        title={pane.title}
        currentPath={pane.path}
        isActive={isActivePane}
      />
      <EverythingBanner />
      <BreadcrumbBar pane={pane} isActive={isActivePane} />
      <HeaderRow pane={pane} />
      {pane.loading ? (
        <div className="min-h-0 flex-1 overflow-auto">
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
        <div ref={parentRef} className="min-h-0 flex-1 overflow-auto" onContextMenu={(event) => showMenu(event)}>
          {/*
            Styling-constraint exception: runtime geometry only. The total
            scroll height and each row's translateY come from
            @tanstack/react-virtual (D18) and are continuous px values that no
            static utility/@theme token can express. Every design-system value
            (color/spacing/typography) elsewhere stays a pure Tailwind utility.
          */}
          <div
            style={{ height: `${totalHeight}px`, position: 'relative' }}
            className="min-h-full"
          >
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
                      onActivate={() => void activateEntry(PARENT_ROW_ID)}
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
                    onActivate={() => void activateEntry(entry.id)}
                    onClick={(event) => selectWithModifiers(entry.id, event)}
                    onMiddleClick={() => void openTabFromPath(paneId, entry.path)}
                    onContextMenu={(event) => showMenu(event, entry.id)}
                  />
                </div>
              )
            })}
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
