import { useMemo } from 'react'
import { useShallow } from 'zustand/react/shallow'
import { formatBytes } from '@/lib/format'
import { isPathInsideVolume } from '@/lib/volumes'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { LoaderCircleIcon } from '@/components/icons'
import { TEXT_SECONDARY } from '@/lib/class-names'
import type { DirectoryEntry, VolumeInfo } from '@/lib/types/ipc'
import type { EntrySizeState, PanesData } from '@/stores/panes/pane-state'

function Dot() {
  return (
    <span aria-hidden="true" className="text-light-text-tertiary dark:text-dark-text-tertiary">
      ·
    </span>
  )
}

function SelectedBytes({ bytes }: { bytes: number | null }) {
  if (bytes === null) {
    return null
  }
  return (
    <>
      <Dot />
      <span>{formatBytes(bytes)}</span>
    </>
  )
}

function FocusedEntryLabel({ entry }: { entry: DirectoryEntry | undefined }) {
  if (!entry) {
    return null
  }
  return (
    <span className="truncate">
      {entry.name}
      {entry.isDir ? ' · folder' : ` · ${entry.typeLabel}`}
    </span>
  )
}

function VolumeSpace({
  volume,
  separated,
}: {
  volume: VolumeInfo | undefined
  separated: boolean
}) {
  if (!volume) {
    return null
  }
  return (
    <>
      {separated ? <Dot /> : null}
      <span>
        {formatBytes(volume.freeBytes)} free of {formatBytes(volume.totalBytes)}
      </span>
    </>
  )
}

function useActivePaneSummary() {
  // Select only cheap references here; the O(n) lookup for the focused entry is
  // deferred to a `useMemo` below so it runs when entries/focus change rather
  // than on every unrelated panes-store emission (size/icon/patch bursts).
  return usePanesStore(
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
}

function indexedSizeStates(state: PanesData, autoFolderSize: boolean) {
  if (!autoFolderSize) {
    return undefined
  }
  if (!state.folderSizeIndexStatus?.isAvailable) {
    return undefined
  }
  return state.sizeStates
}

function isSizeUpdating(size: EntrySizeState | undefined) {
  return size?.state === 'calculating' || size?.refreshing === true
}

// `null` when no selected item has a known size, so an unsized folder
// selection reads as blank rather than "0 B".
function useSelectedBytes(entries: DirectoryEntry[], selectedIds: string[] | undefined) {
  // Folder sizes only count where an index (Everything / the macOS index)
  // keeps them current; otherwise the total is files only.
  const autoFolderSize = useConfigStore((state) => state.autoFolderSize)
  const sizeStates = usePanesStore((state) => indexedSizeStates(state, autoFolderSize))
  return useMemo(() => {
    const selectedIdSet = new Set(selectedIds)
    let total: number | null = null
    for (const entry of entries) {
      const size = entry.isDir ? sizeStates?.[entry.path]?.sizeBytes : entry.sizeBytes
      if (!selectedIdSet.has(entry.id)) continue
      if (size != null) {
        total = (total ?? 0) + size
      }
    }
    return total
  }, [entries, selectedIds, sizeStates])
}

function useSizesUpdating() {
  return usePanesStore((state) =>
    state.panes[state.activePaneId].entries.some((entry) => {
      const size = entry.isDir ? state.sizeStates[entry.path] : undefined
      return isSizeUpdating(size)
    }),
  )
}

function CountingItemsStatus({ counting }: { counting: boolean }) {
  if (!counting) {
    return null
  }
  return (
    <>
      <Dot />
      <span role="status" aria-live="polite" aria-label="Counting items…">
        Counting items…
      </span>
    </>
  )
}

function SizesUpdatingIndicator({ updating }: { updating: boolean }) {
  if (!updating) {
    return null
  }
  return (
    <span className="inline-flex items-center">
      <LoaderCircleIcon className="size-3.5 animate-spin" />
      <span className="sr-only">Updating folder sizes</span>
    </span>
  )
}

export function StatusBar() {
  const activePaneId = usePanesStore((state) => state.activePaneId)
  const { path, typing, itemCount, focusedEntryId, entries, sortKey, itemsSortStatus } =
    useActivePaneSummary()
  const focusedEntry = useMemo(
    () => (focusedEntryId ? entries.find((entry) => entry.id === focusedEntryId) : undefined),
    [entries, focusedEntryId],
  )
  const volumes = usePanesStore(useShallow((state) => state.volumes))
  const selectedIds = useSelectionStore((state) => state.selections[activePaneId]?.selectedIds)
  const selectionCount = selectedIds?.length ?? 0
  // With a multi-selection the focused row's name says nothing useful and
  // would crowd out the selection stats.
  const shownEntry = selectionCount <= 1 ? focusedEntry : undefined
  const selectedBytes = useSelectedBytes(entries, selectedIds)
  const volume = volumes.find((candidate) => isPathInsideVolume(path, candidate.mountRoot))
  const sizesUpdating = useSizesUpdating()

  return (
    <footer
      className={`flex h-status shrink-0 items-center gap-1.5 whitespace-nowrap bg-light-base mica:bg-transparent px-3 text-caption ${TEXT_SECONDARY} dark:bg-dark-base`}
    >
      <span>{itemCount} items</span>
      <Dot />
      <span>{selectionCount} selected</span>
      <SelectedBytes bytes={selectedBytes} />
      <CountingItemsStatus counting={sortKey === 'items' && itemsSortStatus === 'counting'} />
      <Dot />
      <span className="truncate">{typing ? 'Filtering…' : path}</span>
      <div className="flex-1" />
      <FocusedEntryLabel entry={shownEntry} />
      <SizesUpdatingIndicator updating={sizesUpdating} />
      <VolumeSpace volume={volume} separated={shownEntry !== undefined} />
    </footer>
  )
}
