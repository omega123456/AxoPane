import { formatBytes } from '@/lib/format'
import {
  POSIX_ROOT,
  POSIX_SEPARATOR,
  TRAILING_SEPARATORS,
  WINDOWS_DRIVE_PREFIX_LENGTH,
  WINDOWS_SEPARATOR,
} from '@/lib/path-compare'

const SUMMARY_TARGET_LIMIT = 3
const DEFAULT_ARCHIVE_STEM = 'Archive'
const ZIP_EXTENSION = '.zip'

function isWindowsLikePath(path: string) {
  return path.includes(WINDOWS_SEPARATOR) || /^[A-Za-z]:/.test(path)
}

function isRootPath(path: string) {
  return path === POSIX_ROOT || /^[A-Za-z]:[\\/]$/.test(path)
}

function pathSeparatorFor(path: string) {
  return isWindowsLikePath(path) ? WINDOWS_SEPARATOR : POSIX_SEPARATOR
}

export function joinPath(basePath: string, name: string) {
  const separator = pathSeparatorFor(basePath)
  const trimmedBase = /^[A-Za-z]:[\\/]+$/.test(basePath)
    ? `${basePath.slice(0, WINDOWS_DRIVE_PREFIX_LENGTH)}${separator}`
    : basePath.replace(TRAILING_SEPARATORS, '')
  if (trimmedBase === '') {
    return name
  }
  if (isRootPath(trimmedBase)) {
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
