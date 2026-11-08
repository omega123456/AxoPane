import { formatBytes } from '@/lib/format'
import { WINDOWS_DRIVE_PREFIX_LENGTH } from '@/lib/path-compare'

const SUMMARY_TARGET_LIMIT = 3
const DEFAULT_ARCHIVE_STEM = 'Archive'
const ZIP_EXTENSION = '.zip'

function pathSeparatorFor(path: string) {
  return path.includes('\\') || /^[A-Za-z]:/.test(path) ? '\\' : '/'
}

export function joinPath(basePath: string, name: string) {
  const separator = pathSeparatorFor(basePath)
  const trimmedBase = /^[A-Za-z]:[\\/]+$/.test(basePath)
    ? `${basePath.slice(0, WINDOWS_DRIVE_PREFIX_LENGTH)}${separator}`
    : basePath.replace(/[\\/]+$/, '')
  if (trimmedBase === '') {
    return name
  }
  if (trimmedBase === '/' || /^[A-Za-z]:[\\/]$/.test(trimmedBase)) {
    return `${trimmedBase}${name}`
  }
  return `${trimmedBase}${separator}${name}`
}

export function targetSummary(targets: Array<{ name: string; sizeBytes?: number | null }>) {
  const shown = targets
    .slice(0, SUMMARY_TARGET_LIMIT)
    .map((target) =>
      target.sizeBytes == null ? target.name : `${target.name} · ${formatBytes(target.sizeBytes)}`,
    )
  const hiddenCount = targets.length - SUMMARY_TARGET_LIMIT
  return hiddenCount > 0 ? [...shown, `and ${hiddenCount} more`] : shown
}

export function archiveStem(targets: Array<{ name: string }>) {
  if (targets.length !== 1) {
    return DEFAULT_ARCHIVE_STEM
  }

  const name = targets[0].name.trim()
  const dotIndex = name.lastIndexOf('.')
  if (dotIndex > 0) {
    return name.slice(0, dotIndex)
  }
  return name || DEFAULT_ARCHIVE_STEM
}

export function withZipExtension(path: string) {
  return path.toLowerCase().endsWith(ZIP_EXTENSION) ? path : `${path}${ZIP_EXTENSION}`
}

export function itemCountLabel(count: number) {
  return count === 1 ? '1 item' : `${count} items`
}
