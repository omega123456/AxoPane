import type { FolderSizeIndexStatus } from '@/lib/types/ipc'

export const compactCount = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

const PREPARING_LABEL = 'Preparing…'

function pausedLabel(status: FolderSizeIndexStatus) {
  return status.pauseReason === 'thermal'
    ? 'Paused to keep your Mac cool'
    : 'Paused in Low Power Mode'
}

const statusLabels: Record<
  FolderSizeIndexStatus['state'],
  (status: FolderSizeIndexStatus) => string
> = {
  upToDate: () => 'Up to date',
  paused: pausedLabel,
  indexing: () => 'Indexing…',
  updating: () => 'Updating…',
  unavailable: () => PREPARING_LABEL,
  notReady: () => PREPARING_LABEL,
}

/** The macOS index's state in words (Settings and Search everywhere). */
export function folderSizeStatusLabel(status: FolderSizeIndexStatus) {
  const label = statusLabels[status.state] as
    | ((status: FolderSizeIndexStatus) => string)
    | undefined
  return label ? label(status) : PREPARING_LABEL
}
