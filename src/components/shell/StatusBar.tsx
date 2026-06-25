import type { DirectoryEntry, VolumeInfo } from '@/lib/types/ipc'
import type { PaneState } from '@/types/pane'

type StatusBarProps = {
  activePane: PaneState
  summary: {
    itemCount: number
    selectionCount: number
    focusedEntry?: DirectoryEntry
    volume?: VolumeInfo
  }
}

export function StatusBar({ activePane, summary }: StatusBarProps) {
  const volume = summary.volume
  const freeLabel = volume ? formatBytes(volume.freeBytes) : 'Unknown'
  const totalLabel = volume ? formatBytes(volume.totalBytes) : 'Unknown'

  return (
    <footer className="flex h-status items-center gap-3 border-t border-light-border bg-light-titlebar px-4 text-uxs text-light-text-muted dark:border-dark-border dark:bg-dark-titlebar dark:text-dark-text-muted">
      <span>{summary.itemCount} items</span>
      <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
      <span>{summary.selectionCount} selected</span>
      <span className="text-light-text-faint dark:text-dark-text-faint">|</span>
      <span className="truncate">{activePane.typing ? 'Filtering…' : activePane.path}</span>
      <div className="flex-1" />
      {summary.focusedEntry ? (
        <span className="truncate">
          {summary.focusedEntry.name}
          {summary.focusedEntry.isDir ? ' · folder' : ` · ${summary.focusedEntry.typeLabel}`}
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
