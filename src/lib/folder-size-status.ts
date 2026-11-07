import type { FolderSizeIndexStatus } from '@/lib/types/ipc'

export const compactCount = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

/** The macOS index's state in words (Settings and Search everywhere). */
export function folderSizeStatusLabel(status: FolderSizeIndexStatus) {
  switch (status.state) {
    case 'upToDate':
      return 'Up to date'
    case 'paused':
      return status.pauseReason === 'thermal'
        ? 'Paused to keep your Mac cool'
        : 'Paused in Low Power Mode'
    case 'indexing':
      return 'Indexing…'
    case 'updating':
      return 'Updating…'
    default:
      return 'Preparing…'
  }
}
