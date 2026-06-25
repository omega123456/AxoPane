import type { VolumeInfo } from '@/lib/types/ipc'

const windowsDriveRootPattern = /^([A-Za-z]:)[\\/]?$/

export function isPathInsideVolume(path: string, mountRoot: string) {
  const normalizedPath = normalizeExtendedWindowsPath(path).toLowerCase()
  const normalizedRoot = normalizeExtendedWindowsPath(mountRoot).toLowerCase()

  if (normalizedPath === normalizedRoot) {
    return true
  }

  const remainder = normalizedPath.slice(normalizedRoot.length)
  if (!remainder) {
    return false
  }

  return normalizedRoot.endsWith('\\') || normalizedRoot.endsWith('/')
    ? normalizedPath.startsWith(normalizedRoot)
    : remainder.startsWith('\\') || remainder.startsWith('/')
}

function normalizeExtendedWindowsPath(path: string) {
  if (path.toLowerCase().startsWith('\\\\?\\unc\\')) {
    return `\\\\${path.slice(8)}`
  }

  if (/^\\\\\?\\[A-Za-z]:[\\/]/.test(path)) {
    return path.slice(4)
  }

  return path
}

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
