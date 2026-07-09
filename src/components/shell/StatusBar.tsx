import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { isPathInsideVolume } from '@/lib/volumes'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'

export function StatusBar() {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  // Select only cheap references here; the O(n) lookup for the focused entry is
  // deferred to a `useMemo` below so it runs when entries/focus change rather
  // than on every unrelated panes-store emission (size/icon/patch bursts).
  const { path, typing, itemCount, focusedEntryId, entries, sortKey, itemsSortStatus } = usePanesStore(
    useShallow((state) => {
      const pane = state.panes[state.activePaneId]
      return {
        path: pane.path,
        typing: pane.typing,
        itemCount: pane.entries.length,
        focusedEntryId: pane.focusedEntryId,
        entries: pane.entries,
        sortKey: pane.sortKey,
        itemsSortStatus: pane.itemsSortStatus,
      }
    }),
  )
  const focusedEntry = useMemo(
    () => (focusedEntryId ? entries.find((entry) => entry.id === focusedEntryId) : undefined),
    [entries, focusedEntryId],
  )
  const volumes = usePanesStore(useShallow((state) => state.volumes))
  const selectionCount = useSelectionStore(
    (state) => state.selections[activePaneId]?.selectedIds.length ?? 0,
  )
  const volume = volumes.find((candidate) => isPathInsideVolume(path ?? '', candidate.mountRoot))
  const freeLabel = volume ? formatBytes(volume.freeBytes) : 'Unknown'
  const totalLabel = volume ? formatBytes(volume.totalBytes) : 'Unknown'
  const countingItems = sortKey === 'items' && itemsSortStatus === 'counting'

  return (
    <footer className="flex h-status items-center gap-3 border-t border-light-border bg-light-titlebar px-4 text-uxs text-light-text-muted dark:border-dark-border dark:bg-dark-titlebar dark:text-dark-text-muted">
      <span>{itemCount} items</span>
      <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
      <span>{selectionCount} selected</span>
      {countingItems ? (
        <>
          <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
          <span role="status" aria-live="polite" aria-label="Counting items…">Counting items…</span>
        </>
      ) : null}
      <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
      <span className="truncate">{typing ? 'Filtering…' : path}</span>
      <div className="flex-1" />
      {focusedEntry ? (
        <span className="truncate">
          {focusedEntry.name}
          {focusedEntry.isDir ? ' · folder' : ` · ${focusedEntry.typeLabel}`}
        </span>
      ) : null}
      {volume ? (
        <>
          <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
          <span>{freeLabel} free of {totalLabel}</span>
        </>
      ) : null}
    </footer>
  )
}

function formatBytes(value: number) {
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let current = value
  let unitIndex = 0

  while (current >= 1024 && unitIndex < units.length - 1) {
    current /= 1024
    unitIndex += 1
  }

  return `${current.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}
