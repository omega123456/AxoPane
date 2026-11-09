import type { FolderSizeIndexStatus } from '@/lib/types/ipc'

export const compactCount = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

const PREPARING_LABEL = 'Preparing…'

export const INDEX_UNAVAILABLE_LABEL = "The index couldn't start"

export function pauseLabel(pauseReason: FolderSizeIndexStatus['pauseReason']) {
  return pauseReason === 'thermal' ? 'Paused to keep your Mac cool' : 'Paused in Low Power Mode'
}

function pausedLabel(status: FolderSizeIndexStatus) {
  return pauseLabel(status.pauseReason)
}

const statusLabels: Record<
  FolderSizeIndexStatus['state'],
  (status: FolderSizeIndexStatus) => string
> = {
  upToDate: () => 'Up to date',
  paused: pausedLabel,
  indexing: () => 'Indexing…',
  updating: () => 'Updating…',
  unavailable: () => INDEX_UNAVAILABLE_LABEL,
  notReady: () => PREPARING_LABEL,
}

export function folderSizeStatusLabel(status: FolderSizeIndexStatus) {
  const label = statusLabels[status.state] as
    | ((status: FolderSizeIndexStatus) => string)
    | undefined
  return label ? label(status) : PREPARING_LABEL
}

export function isIndexUnavailable(status: FolderSizeIndexStatus) {
  return status.state === 'unavailable'
}
