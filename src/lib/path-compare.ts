const EXTENDED_UNC_PREFIX = '\\\\?\\UNC\\'
const EXTENDED_PATH_PREFIX = '\\\\?\\'
export const UNC_PREFIX = '\\\\'
/** Length of a Windows drive designator such as `C:`. */
export const WINDOWS_DRIVE_PREFIX_LENGTH = 2
const extendedDrivePathPattern = /^\\\\\?\\[A-Za-z]:[\\/]/

export function normalizeWindowsCompatibilityPath(path: string) {
  if (path.toUpperCase().startsWith(EXTENDED_UNC_PREFIX)) {
    return UNC_PREFIX + path.slice(EXTENDED_UNC_PREFIX.length)
  }

  if (extendedDrivePathPattern.test(path)) {
    return path.slice(EXTENDED_PATH_PREFIX.length)
  }

  return path
}

function isWindowsStylePath(path: string) {
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith('\\\\')
}

export type PathComparisonPlatform = 'windows' | 'macos' | 'case-sensitive'

function runtimePathComparisonPlatform(): PathComparisonPlatform {
  if (typeof navigator !== 'undefined' && /mac/i.test(navigator.platform)) return 'macos'
  return 'case-sensitive'
}

/** Exact identity always wins. Windows paths are case-insensitive everywhere;
 * macOS additionally accepts canonical/display case variants. Other POSIX
 * environments deliberately remain case-sensitive. */
export function pathsMatch(
  left: string,
  right: string,
  platform: PathComparisonPlatform = runtimePathComparisonPlatform(),
) {
  if (left === right) {
    return true
  }

  const normalizedLeft = normalizeWindowsCompatibilityPath(left)
  const normalizedRight = normalizeWindowsCompatibilityPath(right)
  if (isWindowsStylePath(normalizedLeft) && isWindowsStylePath(normalizedRight)) {
    return normalizedLeft.toLowerCase() === normalizedRight.toLowerCase()
  }
  return platform === 'macos' && normalizedLeft.toLowerCase() === normalizedRight.toLowerCase()
}

export function samePathOrWindowsCaseFold(left: string, right: string) {
  return pathsMatch(left, right)
}

export function pathKey(
  path: string,
  platform: PathComparisonPlatform = runtimePathComparisonPlatform(),
) {
  const normalized = normalizeWindowsCompatibilityPath(path)
  return isWindowsStylePath(normalized) || platform === 'macos'
    ? normalized.toLowerCase()
    : normalized
}
