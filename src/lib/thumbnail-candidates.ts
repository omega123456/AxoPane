import { getUnixTime, parseISO } from 'date-fns'
import type { DirectoryEntry, ThumbnailCandidateRequest } from '@/lib/types/ipc'

export type GraphicalThumbnailRange = {
  visibleStart: number
  visibleEnd: number
  prefetchStart: number
  prefetchEnd: number
  direction: 'up' | 'down' | 'stationary'
}

type ThumbnailPriority = ThumbnailCandidateRequest['priority']

function isVisibleIndex(index: number, range: GraphicalThumbnailRange) {
  return index >= range.visibleStart && index <= range.visibleEnd
}

function isAheadOfScroll(index: number, range: GraphicalThumbnailRange) {
  return range.direction === 'up' ? index < range.visibleStart : index > range.visibleEnd
}

export function thumbnailPriority(
  index: number,
  range: GraphicalThumbnailRange,
): ThumbnailPriority {
  if (isVisibleIndex(index, range)) {
    return 'visible'
  }
  return isAheadOfScroll(index, range) ? 'ahead' : 'behind'
}

export function distanceFromVisibleRange(index: number, range: GraphicalThumbnailRange): number {
  if (index < range.visibleStart) {
    return range.visibleStart - index
  }
  return isVisibleIndex(index, range) ? index - range.visibleStart : index - range.visibleEnd
}

function modifiedUnixSecondsOf(entry: DirectoryEntry): number | null {
  if (entry.isDir) {
    return null
  }
  if (entry.sizeBytes === null) {
    return null
  }
  if (entry.modifiedAt === null) {
    return null
  }
  const modifiedUnixSeconds = getUnixTime(parseISO(entry.modifiedAt))
  return Number.isFinite(modifiedUnixSeconds) ? modifiedUnixSeconds : null
}

export function thumbnailCandidateFor(
  entry: DirectoryEntry,
  index: number,
  range: GraphicalThumbnailRange,
): ThumbnailCandidateRequest | null {
  const modifiedUnixSeconds = modifiedUnixSecondsOf(entry)
  if (modifiedUnixSeconds === null) {
    return null
  }
  if (entry.sizeBytes === null) {
    return null
  }
  return {
    path: entry.path,
    modifiedUnixSeconds,
    sizeBytes: entry.sizeBytes,
    isDirectory: false,
    priority: thumbnailPriority(index, range),
    order: distanceFromVisibleRange(index, range),
  }
}

export function thumbnailCandidatesInRange(
  entries: DirectoryEntry[],
  range: GraphicalThumbnailRange,
): ThumbnailCandidateRequest[] {
  return entries.slice(range.prefetchStart, range.prefetchEnd + 1).flatMap((entry, offset) => {
    const candidate = thumbnailCandidateFor(entry, range.prefetchStart + offset, range)
    return candidate ? [candidate] : []
  })
}
