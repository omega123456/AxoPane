import type { ReactNode } from 'react'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { LoaderCircleIcon } from '@/components/icons'
import { usePanesStore } from '@/stores/panes-store'
import { EMPTY_VALUE, formatBytes } from '@/lib/format'

type SizeValueProps = {
  entry: DirectoryEntry
}

const DENIED_TITLE = 'macOS blocked access. Grant Full Disk Access to include this folder.'

type DirectorySizeState = ReturnType<typeof usePanesStore.getState>['sizeStates'][string]

const NO_SIZE = <span>—</span>

function CalculatingSize() {
  return (
    <span className="inline-flex items-center justify-end gap-1 text-light-accent dark:text-dark-accent">
      <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
      <span className="sr-only">Calculating size</span>
    </span>
  )
}

function readySize(sizeState: DirectorySizeState) {
  if (sizeState.sizeBytes === null) {
    return NO_SIZE
  }
  if (!sizeState.isLowerBound) {
    return <span>{formatBytes(sizeState.sizeBytes)}</span>
  }
  return (
    <span title={lowerBoundTitle(sizeState.skippedRoots ?? [], sizeState.skippedMore ?? 0)}>
      ≥ {formatBytes(sizeState.sizeBytes)}
    </span>
  )
}

function unavailableSize(sizeState: DirectorySizeState) {
  return <span title={sizeState.naReason === 'denied' ? DENIED_TITLE : undefined}>N/A</span>
}

const directorySizeRenderers: Record<
  DirectorySizeState['state'],
  (sizeState: DirectorySizeState) => ReactNode
> = {
  unknown: () => NO_SIZE,
  calculating: () => <CalculatingSize />,
  ready: readySize,
  na: unavailableSize,
  error: unavailableSize,
}

function DirectorySize({ sizeState }: { sizeState: DirectorySizeState | undefined }) {
  return sizeState ? directorySizeRenderers[sizeState.state](sizeState) : NO_SIZE
}

export function SizeValue({ entry }: SizeValueProps) {
  const sizeState = usePanesStore((state) => state.sizeStates[entry.path])

  if (entry.isDir) {
    return <DirectorySize sizeState={sizeState} />
  }

  return <span>{entry.sizeBytes === null ? EMPTY_VALUE : formatBytes(entry.sizeBytes)}</span>
}

function lowerBoundTitle(names: string[], more: number) {
  if (names.length === 0) {
    return "Doesn't include folders macOS blocked access to."
  }
  const items = more > 0 ? [...names, `${more} more`] : names
  const list =
    items.length === 1
      ? items[0]
      : `${items.slice(0, -1).join(', ')} and ${items[items.length - 1]}`
  return items.length === 1
    ? `Doesn't include ${list}. Open it to include it.`
    : `Doesn't include ${list}. Open them to include them.`
}
