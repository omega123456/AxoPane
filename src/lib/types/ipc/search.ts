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

export type SearchCapabilities = {
  backend: 'everything' | 'macIndex' | 'none'
  foldersOnly: boolean
}

export type SearchHit = {
  path: string
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
  truncated: boolean
  items: SearchHit[]
  capabilities: SearchCapabilities
}

export type SearchRejection =
  | { kind: 'stale' }
  | { kind: 'tooShort' }
  | { kind: 'notReady' }
  | { kind: 'unavailable' }
  | { kind: 'failed'; message: string }
