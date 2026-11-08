import { normalizeWindowsCompatibilityPath } from '@/lib/path-compare'

const UNC_SERVER_ROOT = /^\\\\[^\\/]+[\\/]?$/
const UNC_SHARE_ROOT = /^\\\\[^\\/]+[\\/][^\\/]+[\\/]?$/
const DRIVE_ROOT = /^[A-Za-z]:[\\/]?$/
const DRIVE_PREFIX = /^([A-Za-z]:)[\\/]/
const TRAILING_SEPARATORS = /[\\/]+$/
const POSIX_ROOT = '/'
const WINDOWS_SEPARATOR = '\\'
const POSIX_SEPARATOR = '/'
const NO_SEPARATOR = -1
const POSIX_ROOT_SEPARATOR_INDEX = 0
const DRIVE_LETTER_GROUP = 1

function isTopLevelPath(path: string) {
  return (
    UNC_SERVER_ROOT.test(path) ||
    UNC_SHARE_ROOT.test(path) ||
    DRIVE_ROOT.test(path) ||
    path === POSIX_ROOT
  )
}

function lastSeparatorIndex(path: string) {
  return Math.max(path.lastIndexOf(WINDOWS_SEPARATOR), path.lastIndexOf(POSIX_SEPARATOR))
}

function driveRootParent(stripped: string, lastSeparator: number) {
  const drive = stripped.match(DRIVE_PREFIX)?.[DRIVE_LETTER_GROUP]
  if (drive === undefined) {
    return null
  }
  const separatorFollowsDrive = lastSeparator === drive.length
  return separatorFollowsDrive ? `${drive}${WINDOWS_SEPARATOR}` : null
}

export function getParentPath(path: string): string | null {
  if (!path) {
    return null
  }

  const normalized = normalizeWindowsCompatibilityPath(path)
  if (isTopLevelPath(normalized)) {
    return null
  }

  const stripped = normalized.replace(TRAILING_SEPARATORS, '')
  const lastSeparator = lastSeparatorIndex(stripped)
  if (lastSeparator === NO_SEPARATOR) {
    return null
  }

  const driveRoot = driveRootParent(stripped, lastSeparator)
  if (driveRoot) {
    return driveRoot
  }

  const parentIsPosixRoot = lastSeparator === POSIX_ROOT_SEPARATOR_INDEX
  if (parentIsPosixRoot) {
    return POSIX_ROOT
  }

  return stripped.slice(0, lastSeparator)
}

export function basenameFromPath(path: string): string {
  const normalized = normalizeWindowsCompatibilityPath(path)
  const stripped = normalized.replace(TRAILING_SEPARATORS, '')
  const lastSeparator = lastSeparatorIndex(stripped)
  const hasNoFinalSegment = lastSeparator === NO_SEPARATOR || lastSeparator === stripped.length - 1
  if (hasNoFinalSegment) {
    return stripped || normalized
  }
  return stripped.slice(lastSeparator + 1)
}
