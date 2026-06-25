import type { DirectoryEntry } from '@/lib/types/ipc'
import { LoaderCircleIcon } from '@/components/icons'
import { usePanesStore } from '@/stores/panes-store'

type SizeValueProps = {
  entry: DirectoryEntry
}

export function SizeValue({ entry }: SizeValueProps) {
  const sizeState = usePanesStore((state) => state.sizeStates[entry.path])

  if (entry.isDir) {
    if (sizeState?.state === 'calculating') {
      return (
        <span className="inline-flex items-center justify-end gap-1 text-accent-blue-light dark:text-accent-blue">
          <LoaderCircleIcon className="h-3.5 w-3.5 animate-spin" />
        </span>
      )
    }

    if (sizeState?.state === 'ready' && sizeState.sizeBytes !== null) {
      return <span>{formatBytes(sizeState.sizeBytes)}</span>
    }

    if (sizeState?.state === 'na' || sizeState?.state === 'error') {
      return <span>N/A</span>
    }

    return <span>—</span>
  }

  return <span>{entry.sizeBytes === null ? '—' : formatBytes(entry.sizeBytes)}</span>
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
