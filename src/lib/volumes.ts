import type { VolumeCategory, VolumeGroup, VolumeInfo } from '@/lib/types/ipc'
import {
  normalizeWindowsCompatibilityPath,
  pathKey,
  POSIX_SEPARATOR,
  WINDOWS_SEPARATOR,
} from '@/lib/path-compare'
import { clamp } from '@/lib/numbers'
import { compareNaturally } from '@/lib/natural-order'

const windowsDriveRootPattern = /^([A-Za-z]:)[\\/]?$/

export function isPathInsideVolume(path: string, mountRoot: string) {
  const normalizedPath = pathKey(normalizeWindowsCompatibilityPath(path))
  const normalizedRoot = pathKey(normalizeWindowsCompatibilityPath(mountRoot))

  if (normalizedPath === normalizedRoot) {
    return true
  }

  const remainder = normalizedPath.slice(normalizedRoot.length)
  if (!remainder) {
    return false
  }

  return endsWithSeparator(normalizedRoot)
    ? normalizedPath.startsWith(normalizedRoot)
    : startsWithSeparator(remainder)
}

function endsWithSeparator(path: string) {
  return path.endsWith(WINDOWS_SEPARATOR) || path.endsWith(POSIX_SEPARATOR)
}

function startsWithSeparator(path: string) {
  return path.startsWith(WINDOWS_SEPARATOR) || path.startsWith(POSIX_SEPARATOR)
}

function isDeeperMount(candidate: VolumeInfo, current: VolumeInfo | null) {
  if (!current) {
    return true
  }
  return candidate.mountRoot.length > current.mountRoot.length
}

function normalizeRoot(path: string) {
  const normalized = normalizeWindowsCompatibilityPath(path).replace(/[\\/]+$/, '')
  return /^[A-Za-z]:$/.test(normalized) ? normalized.toLowerCase() : pathKey(normalized)
}

export function findVolumeForPath(path: string, volumes: VolumeInfo[]): VolumeInfo | null {
  let match: VolumeInfo | null = null
  for (const volume of volumes) {
    if (!isPathInsideVolume(path, volume.mountRoot)) {
      continue
    }
    if (isDeeperMount(volume, match)) {
      match = volume
    }
  }
  return match
}

export function isNetworkPath(path: string, volumes: VolumeInfo[]): boolean {
  const volume = findVolumeForPath(path, volumes)
  if (volume) {
    return volume.isNetwork
  }

  return normalizeWindowsCompatibilityPath(path).startsWith('\\\\')
}

export function isVolumeRoot(path: string, volume: VolumeInfo) {
  return normalizeRoot(path) === normalizeRoot(volume.mountRoot)
}

export function sortVolumesForTree(volumes: VolumeInfo[]) {
  return [...volumes].sort((left, right) => compareNaturally(left.mountRoot, right.mountRoot))
}

export function volumeCategory(volume: VolumeInfo): VolumeCategory {
  if (volume.isNetwork) {
    return 'network'
  }

  if (volume.isRemovable) {
    return 'removable'
  }

  return 'fixed'
}

const categoryOrder: { category: VolumeCategory; label: string }[] = [
  { category: 'fixed', label: 'Drives' },
  { category: 'removable', label: 'Removable' },
  { category: 'network', label: 'Network' },
]

export function groupVolumesByCategory(volumes: VolumeInfo[]): VolumeGroup[] {
  const sorted = sortVolumesForTree(volumes)

  return categoryOrder
    .map(({ category, label }) => ({
      category,
      label,
      volumes: sorted.filter((volume) => volumeCategory(volume) === category),
    }))
    .filter((group) => group.volumes.length > 0)
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

  const labelIsDrive = trimmedLabel.localeCompare(drive, undefined, { sensitivity: 'base' }) === 0
  if (labelIsDrive) {
    return drive
  }

  return `${trimmedLabel} (${drive})`
}

export type CapacityLevel = 'normal' | 'warning' | 'critical'

export function volumeUsedFraction(volume: VolumeInfo): number | null {
  if (volume.isNetwork) {
    return null
  }
  if (volume.totalBytes <= 0) {
    return null
  }
  const used = (volume.totalBytes - volume.freeBytes) / volume.totalBytes
  return clamp(used, 0, 1)
}

const CAPACITY_CRITICAL_FRACTION = 0.95
const CAPACITY_WARNING_FRACTION = 0.85

export function capacityLevel(usedFraction: number): CapacityLevel {
  if (usedFraction >= CAPACITY_CRITICAL_FRACTION) {
    return 'critical'
  }
  return usedFraction >= CAPACITY_WARNING_FRACTION ? 'warning' : 'normal'
}
