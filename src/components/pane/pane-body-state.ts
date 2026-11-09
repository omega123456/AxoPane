const PARENT_ROW_COUNT = 1
const NO_PARENT_ROW = 0

export function parentRowOffset(hasParent: boolean) {
  return hasParent ? PARENT_ROW_COUNT : NO_PARENT_ROW
}

export type PaneBodyState = 'skeleton' | 'permissionDenied' | 'error' | 'empty' | 'listing'

const PERMISSION_ERROR_PATTERN = /permission|denied|forbidden|access is denied/i

export function isPermissionError(message: string | null) {
  return message ? PERMISSION_ERROR_PATTERN.test(message) : false
}

/**
 * Which placeholder replaces the listing, if any. An empty folder still shows
 * the synthetic ".." row when a parent exists, so `empty` only applies when
 * there is genuinely nothing to render (an empty folder at a filesystem root).
 */
export function paneBodyState({
  showSkeleton,
  loading,
  error,
  rowCount,
}: {
  showSkeleton: boolean
  loading: boolean
  error: string | null
  rowCount: number
}): PaneBodyState {
  if (showSkeleton) {
    return 'skeleton'
  }
  if (isPermissionError(error)) {
    return 'permissionDenied'
  }
  if (error) {
    return 'error'
  }
  const finishedWithoutRows = !loading && rowCount === 0
  return finishedWithoutRows ? 'empty' : 'listing'
}
