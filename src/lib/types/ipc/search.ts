export type SearchScope = 'all' | 'folders' | 'files'

export type SearchQueryRequest = {
  query: string
  includeHidden: boolean
  scope: SearchScope
}

export type SearchPageRequest = {
  queryId: number
  offset: number
}

/** Which backend answers Search everywhere. */
export type SearchCapabilities = {
  backend: 'everything' | 'macIndex' | 'none'
  /** The backend knows folder names only (the macOS index). */
  foldersOnly: boolean
}

export type SearchHit = {
  /** The backend's path; every action uses it. */
  path: string
  /** Display name and parent; the ranges are `[start, end)` UTF-16 offsets into them. */
  name: string
  parentPath: string
  isDir: boolean
  modifiedAt: string | null
  nameRanges: [number, number][]
  parentRanges: [number, number][]
}

export type SearchPage = {
  queryId: number
  offset: number
  total: number
  /** The backend had more matches than were ranked. */
  truncated: boolean
  items: SearchHit[]
  capabilities: SearchCapabilities
}

/** Mirrors Rust's `#[serde(tag = "kind")] SearchRejection`. */
export type SearchRejection =
  | { kind: 'stale' }
  | { kind: 'tooShort' }
  | { kind: 'notReady' }
  | { kind: 'unavailable' }
  | { kind: 'failed'; message: string }
