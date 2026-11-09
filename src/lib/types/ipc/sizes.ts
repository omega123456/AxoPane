import type { DirectoryEntry } from './listing'

export type DirPatchEvent = {
  tabId: string
  path: string
  reason: 'refresh' | 'watch'
  changed: { path: string; entry: DirectoryEntry | null }[]
  removed: string[]
}

export type SizeStateEvent = {
  path: string
  state: 'unknown' | 'calculating' | 'ready' | 'error' | 'na'
  source: 'everything' | 'manual' | 'network' | 'index'
  sizeBytes: number | null
  naReason?: 'network' | 'timeout' | 'denied'
  isLowerBound?: boolean
  skippedRoots?: string[]
  skippedMore?: number
}

export type FolderSizeIndexStatus = {
  backend: 'everything' | 'macIndex' | 'none'
  isAvailable: boolean
  state: 'unavailable' | 'notReady' | 'indexing' | 'updating' | 'upToDate' | 'paused'
  itemsScanned?: number
  pauseReason?: 'lowPowerMode' | 'thermal'
}
export type SizeRequestMode = 'auto' | 'manual' | 'refresh'

export type FolderSizeRequest = {
  path: string
  mode?: SizeRequestMode
}

export type FolderSizesRequest = {
  paths: string[]
  mode?: SizeRequestMode
}

export type SizeSubscriptionsRequest = {
  paneId: string
  currentPath: string | null
  folderPaths: string[]
}

export type RequestIconsRequest = {
  paths: string[]
}

export type IconStateEvent = {
  path: string
  iconDataUrl: string | null
}

export type ThumbnailCandidateRequest = {
  path: string
  modifiedUnixSeconds: number
  sizeBytes: number
  isDirectory: boolean
  priority: 'visible' | 'ahead' | 'behind'
  order: number
}

export type RequestThumbnailsRequest = {
  paneId: string
  tabId: string
  path: string
  generation: number
  revision: number
  candidates: ThumbnailCandidateRequest[]
}

export type RequestThumbnailsResponse = {
  revision: number
  acceptedCount: number
}

export type CancelThumbnailsRequest = {
  paneId: string
  tabId: string
  path: string
  generation: number
}

export type ThumbnailResultKind = 'ready' | 'unavailable' | 'failed'
export type ThumbnailQuality = 'low' | 'high'

export type ThumbnailResultEvent = {
  paneId: string
  tabId: string
  path: string
  generation: number
  fingerprintPath: string
  modifiedUnixSeconds: number
  sizeBytes: number
  state: ThumbnailResultKind
  quality: ThumbnailQuality | null
  dataUrl: string | null
}

export type CancelSizeRequest = {
  path: string
}

export type CancelSizeResponse = {
  cancelled: boolean
}

export type CancelSizesRequest = {
  paths: string[]
}

export type CancelSizesResponse = {
  cancelled: number
}
