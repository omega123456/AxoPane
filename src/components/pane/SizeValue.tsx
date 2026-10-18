import type { DirectoryEntry } from '@/lib/types/ipc'
import { LoaderCircleIcon } from '@/components/icons'
import { usePanesStore } from '@/stores/panes-store'

type SizeValueProps = {
  entry: DirectoryEntry
}

const DENIED_TITLE = 'macOS blocked access. Grant Full Disk Access to include this folder.'

export function SizeValue({ entry }: SizeValueProps) {
  const sizeState = usePanesStore((state) => state.sizeStates[entry.path])

  if (entry.isDir) {
    if (sizeState?.state === 'calculating') {
      return (
        <span className="inline-flex items-center justify-end gap-1 text-light-accent dark:text-dark-accent">
          <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
          <span className="sr-only">Calculating size</span>
        </span>
      )
    }

    if (sizeState?.state === 'ready' && sizeState.sizeBytes !== null) {
      if (sizeState.isLowerBound) {
        return (
          <span title={lowerBoundTitle(sizeState.skippedRoots ?? [], sizeState.skippedMore ?? 0)}>
            ≥ {formatBytes(sizeState.sizeBytes)}
          </span>
        )
      }
      return <span>{formatBytes(sizeState.sizeBytes)}</span>
    }

    if (sizeState?.state === 'na' || sizeState?.state === 'error') {
      return <span title={sizeState.naReason === 'denied' ? DENIED_TITLE : undefined}>N/A</span>
    }

    return <span>—</span>
  }

  return <span>{entry.sizeBytes === null ? '—' : formatBytes(entry.sizeBytes)}</span>
}

/** "Doesn't include Documents, Desktop, Downloads and 2 more. Open them to include them." */
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
