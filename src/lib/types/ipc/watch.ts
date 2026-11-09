import type { DirectoryEntry, SortDirection, SortKey, VolumeInfo } from './listing'

export type WatchTarget = {
  tabId: string
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
  includeItemCounts: boolean
}

export type WatchSeedReference = {
  tabId: string
  requestId: number
  path: string
}

export type ItemCountRequestContext = {
  paneId: string
  tabId: string
  requestId: number
  path: string
}

export type VisibleItemCountsRequest = {
  context: ItemCountRequestContext
  paths: string[]
}

export type ItemCountResult = {
  path: string
  itemCount: number | null
}

export type ItemCountEvent = {
  context: ItemCountRequestContext
  results: ItemCountResult[]
  done: boolean
}

export type ActiveItemsSortRequest = {
  context: ItemCountRequestContext
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
}

export type ActiveItemsSortReady = {
  kind: 'ready'
  context: ItemCountRequestContext
  path: string
  entries: DirectoryEntry[]
}

export type ActiveItemsSortSuperseded = {
  kind: 'superseded'
  context: ItemCountRequestContext
}

export type ActiveItemsSortResponse = ActiveItemsSortReady | ActiveItemsSortSuperseded

export type SetTabWatchRequest = {
  target: WatchTarget | null
  seedReference?: WatchSeedReference
  entries?: DirectoryEntry[]
}

export type VolumesChangedEvent = {
  volumes: VolumeInfo[]
}
