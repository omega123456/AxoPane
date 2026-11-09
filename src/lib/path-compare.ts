const EXTENDED_UNC_PREFIX = '\\\\?\\UNC\\'
const EXTENDED_PATH_PREFIX = '\\\\?\\'
export const UNC_PREFIX = '\\\\'
export const WINDOWS_SEPARATOR = '\\'
export const POSIX_SEPARATOR = '/'
export const POSIX_ROOT = '/'
export const CURRENT_DIRECTORY = '.'
export const ANY_SEPARATOR = /[\\/]/
export const TRAILING_SEPARATORS = /[\\/]+$/
export const UNC_SHARE_PREFIX = /^\\\\([^\\/]+)[\\/]([^\\/]+)[\\/]?/
export const DRIVE_PREFIX = /^([A-Za-z]:)[\\/]?/
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
  return /^[A-Za-z]:[\\/]/.test(path) || path.startsWith(UNC_PREFIX)
}

export type PathComparisonPlatform = 'windows' | 'macos' | 'case-sensitive'

function isMacNavigator() {
  return typeof navigator !== 'undefined' && /mac/i.test(navigator.platform)
}

function runtimePathComparisonPlatform(): PathComparisonPlatform {
  if (isMacNavigator()) return 'macos'
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
  const bothWindowsStyle = isWindowsStylePath(normalizedLeft) && isWindowsStylePath(normalizedRight)
  if (bothWindowsStyle) {
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
  const foldsCase = isWindowsStylePath(normalized) || platform === 'macos'
  return foldsCase ? normalized.toLowerCase() : normalized
}
