import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatBytes } from '@/lib/format'
import { isPathInsideVolume } from '@/lib/volumes'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { LoaderCircleIcon } from '@/components/icons'

function Dot() {
  return (
    <span aria-hidden="true" className="text-light-text-tertiary dark:text-dark-text-tertiary">
      ·
    </span>
  )
}

export function StatusBar() {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  // Select only cheap references here; the O(n) lookup for the focused entry is
  // deferred to a `useMemo` below so it runs when entries/focus change rather
  // than on every unrelated panes-store emission (size/icon/patch bursts).
  const { path, typing, itemCount, focusedEntryId, entries, sortKey, itemsSortStatus } =
    usePanesStore(
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
  const selectedIds = useSelectionStore((state) => state.selections[activePaneId]?.selectedIds)
  const selectionCount = selectedIds?.length ?? 0
  // Folder sizes only count where an index (Everything / the macOS index)
  // keeps them current; otherwise the total is files only.
  const autoFolderSize = useConfigStore((state) => state.autoFolderSize)
  const sizeStates = usePanesStore((state) =>
    autoFolderSize && state.folderSizeIndexStatus?.isAvailable ? state.sizeStates : undefined,
  )
  // With a multi-selection the focused row's name says nothing useful and
  // would crowd out the selection stats.
  const shownEntry = selectionCount <= 1 ? focusedEntry : undefined
  // `null` when no selected item has a known size, so an unsized folder
  // selection reads as blank rather than "0 B".
  const selectedBytes = useMemo(() => {
    const ids = new Set(selectedIds)
    let total: number | null = null
    for (const entry of entries) {
      const size = entry.isDir ? sizeStates?.[entry.path]?.sizeBytes : entry.sizeBytes
      if (ids.has(entry.id) && size != null) {
        total = (total ?? 0) + size
      }
    }
    return total
  }, [entries, selectedIds, sizeStates])
  const volume = volumes.find((candidate) => isPathInsideVolume(path ?? '', candidate.mountRoot))
  const freeLabel = volume ? formatBytes(volume.freeBytes) : 'Unknown'
  const totalLabel = volume ? formatBytes(volume.totalBytes) : 'Unknown'
  const countingItems = sortKey === 'items' && itemsSortStatus === 'counting'
  const sizesUpdating = usePanesStore((state) =>
    state.panes[state.activePaneId].entries.some((entry) => {
      const size = entry.isDir ? state.sizeStates[entry.path] : undefined
      return size?.state === 'calculating' || size?.refreshing === true
    }),
  )

  return (
    <footer className="flex h-status shrink-0 items-center gap-1.5 whitespace-nowrap bg-light-base mica:bg-transparent px-3 text-caption text-light-text-secondary dark:bg-dark-base dark:text-dark-text-secondary">
      <span>{itemCount} items</span>
      <Dot />
      <span>{selectionCount} selected</span>
      {selectedBytes !== null ? (
        <>
          <Dot />
          <span>{formatBytes(selectedBytes)}</span>
        </>
      ) : null}
      {countingItems ? (
        <>
          <Dot />
          <span role="status" aria-live="polite" aria-label="Counting items…">
            Counting items…
          </span>
        </>
      ) : null}
      <Dot />
      <span className="truncate">{typing ? 'Filtering…' : path}</span>
      <div className="flex-1" />
      {shownEntry ? (
        <span className="truncate">
          {shownEntry.name}
          {shownEntry.isDir ? ' · folder' : ` · ${shownEntry.typeLabel}`}
        </span>
      ) : null}
      {sizesUpdating ? (
        <span className="inline-flex items-center">
          <LoaderCircleIcon className="size-3.5 animate-spin" />
          <span className="sr-only">Updating folder sizes</span>
        </span>
      ) : null}
      {volume ? (
        <>
          {shownEntry ? <Dot /> : null}
          <span>
            {freeLabel} free of {totalLabel}
          </span>
        </>
      ) : null}
    </footer>
  )
}
