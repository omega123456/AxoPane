export function normalizeWindowsCompatibilityPath(path: string) {
  if (path.startsWith('\\\\?\\UNC\\')) {
    return `\\\\${path.slice(8)}`
  }

  if (/^\\\\\?\\[A-Za-z]:[\\/]/.test(path)) {
    return path.slice(4)
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

export function pathKey(path: string, platform: PathComparisonPlatform = runtimePathComparisonPlatform()) {
  const normalized = normalizeWindowsCompatibilityPath(path)
  return isWindowsStylePath(normalized) || platform === 'macos' ? normalized.toLowerCase() : normalized
}
