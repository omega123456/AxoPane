import { DETAIL_SEPARATOR } from '@/lib/format'
import { compactCount, pauseLabel } from '@/lib/folder-size-status'
import type { FolderSizeIndexStatus, SearchCapabilities, SearchHit } from '@/lib/types/ipc'
import { getParentPath } from '@/stores/panes-store'
import { isSearchable, type SearchStatus } from '@/stores/search-palette-store'
import { POSIX_SEPARATOR, WINDOWS_SEPARATOR } from '@/lib/path-compare'

const TRAILING_SEPARATOR_PATTERN = /[\\/]$/

export type SearchBackend = SearchCapabilities['backend']

export type PalettePhase = 'complete' | 'indexing' | 'paused' | 'updating' | 'fallback'

export type SearchBackendInfo = SearchCapabilities & { phase: PalettePhase }

export type SearchFooterInput = {
  backend: SearchBackend
  phase: PalettePhase
  status: FolderSizeIndexStatus | null
  searching: boolean
  loaded: number
  total: number
  truncated: boolean
}

export const FILE_NAMES_READY_ANNOUNCEMENT = 'File names are ready.'

export function folderRow(path: string, name: string): SearchHit {
  return {
    path,
    name,
    parentPath: getParentPath(path) ?? '',
    isDir: true,
    modifiedAt: null,
    nameRanges: [],
    parentRanges: [],
  }
}

export function withTrailingSeparator(path: string) {
  if (TRAILING_SEPARATOR_PATTERN.test(path)) {
    return path
  }
  const separator = path.includes(WINDOWS_SEPARATOR) ? WINDOWS_SEPARATOR : POSIX_SEPARATOR
  return `${path}${separator}`
}

function resultCountText(loaded: number, total: number) {
  if (loaded < total) {
    return `Showing ${loaded} of ${total}`
  }
  return `${total} ${total === 1 ? 'result' : 'results'}`
}

function resultsSuffix({ searching, loaded, total, truncated }: SearchFooterInput) {
  if (!searching) {
    return ''
  }
  const refineHint = truncated ? `${DETAIL_SEPARATOR}refine your search` : ''
  return `${DETAIL_SEPARATOR}${resultCountText(loaded, total)}${refineHint}`
}

function indexingFooter(status: FolderSizeIndexStatus | null) {
  const itemsScanned = status?.itemsScanned
  const scanned = itemsScanned === undefined ? '' : ` ${compactCount.format(itemsScanned)} items`
  return `Indexing files…${scanned}`
}

type MacIndexFooter = (status: FolderSizeIndexStatus | null, results: string) => string

const macIndexFooters: Record<PalettePhase, MacIndexFooter> = {
  indexing: (status) => indexingFooter(status),
  paused: (status) => pauseLabel(status?.pauseReason),
  fallback: (_status, results) => `Folders on this Mac${results}`,
  updating: (_status, results) => `Updating index…${results}`,
  complete: (_status, results) => `Files and folders on this Mac${results}`,
}

export function footerText(input: SearchFooterInput) {
  const results = resultsSuffix(input)
  if (input.backend !== 'macIndex') {
    return `Everything${results}`
  }
  return macIndexFooters[input.phase](input.status, results)
}

function isFoldersOnly(
  capabilities: SearchCapabilities | null,
  indexStatus: FolderSizeIndexStatus | null,
) {
  if (indexStatus?.backend === 'macIndex') {
    return !indexStatus.fileNamesAvailable
  }
  return capabilities?.foldersOnly ?? false
}

function macIndexPhase(status: FolderSizeIndexStatus | null, foldersOnly: boolean): PalettePhase {
  const state = status?.state
  if (state === 'indexing') {
    return 'indexing'
  }
  if (state === 'paused') {
    return 'paused'
  }
  if (foldersOnly) {
    return 'fallback'
  }
  if (state === 'updating') {
    return 'updating'
  }
  return 'complete'
}

export function searchBackendInfo(
  capabilities: SearchCapabilities | null,
  indexStatus: FolderSizeIndexStatus | null,
): SearchBackendInfo {
  const backend = capabilities?.backend ?? indexStatus?.backend ?? 'none'
  const foldersOnly = isFoldersOnly(capabilities, indexStatus)
  if (backend !== 'macIndex') {
    return { backend, foldersOnly, phase: 'complete' }
  }
  return { backend, foldersOnly, phase: macIndexPhase(indexStatus, foldersOnly) }
}

export function lastRenderedIndex(virtualRows: { index: number }[]) {
  return virtualRows.at(-1)?.index ?? -1
}

const phaseScopeNotes: Record<PalettePhase, string> = {
  indexing: 'Results may be incomplete until indexing finishes.',
  paused: 'Results may be incomplete until indexing resumes.',
  fallback: 'File names aren’t available on this Mac right now.',
  updating: '',
  complete: '',
}

export function scopeNoteFor(searching: boolean, query: string, phase: PalettePhase) {
  const shortQuery = !searching && query.trim().length > 0 && !isSearchable(query)
  if (shortQuery) {
    return 'Keep typing. Search starts at 3 characters'
  }
  return phaseScopeNotes[phase]
}

function finishedIndexingWithNames(
  previous: FolderSizeIndexStatus | null,
  next: FolderSizeIndexStatus | null,
) {
  const wasIndexing = previous?.state === 'indexing'
  const stillIndexing = next?.state === 'indexing'
  const namesAvailable = next?.fileNamesAvailable === true
  return wasIndexing && !stillIndexing && namesAvailable
}

export function indexAnnouncementAfter(
  previous: FolderSizeIndexStatus | null,
  next: FolderSizeIndexStatus | null,
  current: string,
) {
  if (finishedIndexingWithNames(previous, next)) {
    return FILE_NAMES_READY_ANNOUNCEMENT
  }
  if (next?.state === 'indexing') {
    return ''
  }
  return current
}

export function resultAnnouncement(status: SearchStatus, total: number) {
  if (status !== 'ready') {
    return ''
  }
  return total === 0 ? 'No results' : `${total} results`
}

export function inputPlaceholder(foldersOnly: boolean) {
  return foldersOnly
    ? 'Search folders, or type a path like alex/'
    : 'Search files and folders, or type a path like alex/'
}

export type PaletteSuggestions = {
  favouriteRows: SearchHit[]
  recentRows: SearchHit[]
}

export function paletteRows(
  searching: boolean,
  items: SearchHit[],
  { favouriteRows, recentRows }: PaletteSuggestions,
) {
  return searching ? items : [...favouriteRows, ...recentRows]
}
