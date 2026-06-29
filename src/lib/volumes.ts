import type { VolumeCategory, VolumeGroup, VolumeInfo } from '@/lib/types/ipc'

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

/** Strip trailing separators and lowercase, for root-equality comparisons. */
function normalizeRoot(path: string) {
  return normalizeExtendedWindowsPath(path).replace(/[\\/]+$/, '').toLowerCase()
}

/**
 * The volume a path lives in, or `null` when none match. When several volumes
 * contain the path (e.g. a mounted volume nested under another), the deepest
 * mount root wins.
 */
export function findVolumeForPath(path: string, volumes: VolumeInfo[]): VolumeInfo | null {
  let match: VolumeInfo | null = null
  for (const volume of volumes) {
    if (!isPathInsideVolume(path, volume.mountRoot)) {
      continue
    }
    if (!match || volume.mountRoot.length > match.mountRoot.length) {
      match = volume
    }
  }
  return match
}

/** True when `path` is itself a volume's mount root (not a folder inside it). */
export function isVolumeRoot(path: string, volume: VolumeInfo) {
  return normalizeRoot(path) === normalizeRoot(volume.mountRoot)
}

export function sortVolumesForTree(volumes: VolumeInfo[]) {
  return [...volumes].sort((left, right) =>
    left.mountRoot.localeCompare(right.mountRoot, undefined, {
      numeric: true,
      sensitivity: 'base',
    }),
  )
}

// Network mounts win over the removable flag (a mapped network drive can report
// as removable on some systems but belongs under Network Drives), so the order
// of these checks matters.
export function volumeCategory(volume: VolumeInfo): VolumeCategory {
  if (volume.isNetwork) {
    return 'network'
  }

  if (volume.isRemovable) {
    return 'removable'
  }

  return 'fixed'
}

// Tree section order + headings. Only non-empty sections are rendered.
const categoryOrder: { category: VolumeCategory; label: string }[] = [
  { category: 'fixed', label: 'Drives' },
  { category: 'removable', label: 'Removable Drives' },
  { category: 'network', label: 'Network Drives' },
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

  if (trimmedLabel.localeCompare(drive, undefined, { sensitivity: 'base' }) === 0) {
    return drive
  }

  return `${trimmedLabel} (${drive})`
}
