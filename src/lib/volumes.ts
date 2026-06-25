import type { VolumeInfo } from '@/lib/types/ipc'

const windowsDriveRootPattern = /^([A-Za-z]:)[\\/]?$/

export function sortVolumesForTree(volumes: VolumeInfo[]) {
  return [...volumes].sort((left, right) =>
    left.mountRoot.localeCompare(right.mountRoot, undefined, {
      numeric: true,
      sensitivity: 'base',
    }),
  )
}

export function formatVolumeTreeName(volume: VolumeInfo) {
  const trimmedLabel = volume.label.trim()
  const driveMatch = volume.mountRoot.match(windowsDriveRootPattern)

  if (!driveMatch) {
    return trimmedLabel || volume.mountRoot
  }

  const drive = driveMatch[1]
  if (!trimmedLabel) {
    return drive
  }

  if (trimmedLabel.localeCompare(drive, undefined, { sensitivity: 'base' }) === 0) {
    return drive
  }

  return `${trimmedLabel} (${drive})`
}
