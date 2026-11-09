import { DETAIL_SEPARATOR } from '@/lib/format'
import { compactCount, folderSizeStatusLabel } from '@/lib/folder-size-status'
import type { FolderSizeIndexStatus, SearchCapabilities, SearchHit } from '@/lib/types/ipc'
import { getParentPath } from '@/stores/panes-store'
import { isSearchable, type SearchStatus } from '@/stores/search-palette-store'

const TRAILING_SEPARATOR_PATTERN = /[\\/]$/
const WINDOWS_SEPARATOR = '\\'
const POSIX_SEPARATOR = '/'

export type SearchBackend = SearchCapabilities['backend']

export type SearchFooterInput = {
  backend: SearchBackend
  status: FolderSizeIndexStatus | null
  searching: boolean
  loaded: number
  total: number
  truncated: boolean
}

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

function indexingFooter(status: FolderSizeIndexStatus) {
  const scanned =
    status.itemsScanned === undefined ? '' : ` ${compactCount.format(status.itemsScanned)} folders`
  return `Indexing…${scanned}${DETAIL_SEPARATOR}results may be incomplete`
}

const macIndexStateFooters: Partial<
  Record<FolderSizeIndexStatus['state'], (status: FolderSizeIndexStatus) => string>
> = {
  indexing: indexingFooter,
  paused: folderSizeStatusLabel,
}

function macIndexFooter(status: FolderSizeIndexStatus | null, results: string) {
  const foldersFooter = `Folders on this Mac${results}`
  if (!status) {
    return foldersFooter
  }
  const stateFooter = macIndexStateFooters[status.state]
  return stateFooter ? stateFooter(status) : foldersFooter
}

export function footerText(input: SearchFooterInput) {
  const results = resultsSuffix(input)
  if (input.backend !== 'macIndex') {
    return `Everything${results}`
  }
  return macIndexFooter(input.status, results)
}

export function searchBackendInfo(
  capabilities: SearchCapabilities | null,
  indexStatus: FolderSizeIndexStatus | null,
): SearchCapabilities {
  const backend = capabilities?.backend ?? indexStatus?.backend ?? 'none'
  return { backend, foldersOnly: capabilities?.foldersOnly ?? backend === 'macIndex' }
}

export function lastRenderedIndex(virtualRows: { index: number }[]) {
  return virtualRows.at(-1)?.index ?? -1
}

export function scopeNoteFor(searching: boolean, query: string, foldersOnly: boolean) {
  const shortQuery = !searching && query.trim().length > 0 && !isSearchable(query)
  if (shortQuery) {
    return 'Keep typing. Search starts at 3 characters'
  }
  return foldersOnly ? 'Folder names only on this Mac' : ''
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
