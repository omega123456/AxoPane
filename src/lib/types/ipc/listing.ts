export type DirectoryEntry = {
  id: string
  name: string
  path: string
  isDir: boolean
  iconDataUrl?: string | null
  sizeBytes: number | null
  itemCount: number | null
  typeLabel: string
  modifiedAt: string | null
  createdAt: string | null
  attributes: string[]
  isHidden: boolean
  isSystem: boolean
  /** Set only for rows synthesized from a trash listing: the opaque id used to restore/purge this item. */
  trashId?: string
  /** Set only for rows synthesized from a trash listing, when the original location is known. */
  originalPath?: string | null
}

export type SortDirection = 'asc' | 'desc'
export type SortKey = 'name' | 'size' | 'items' | 'type' | 'modified' | 'created'

export type ListDirRequest = {
  path: string
  sortKey: SortKey
  sortDirection: SortDirection
  filter: string
  showHidden: boolean
  includeItemCounts: boolean
}

export type ListDirResponse = {
  path: string
  entries: DirectoryEntry[]
}

export type TreeChildEntry = {
  name: string
  path: string
  expandability: TreeExpandability
}

export type TreeExpandability = 'unknown' | 'empty' | 'nonEmpty'

export type ListTreeChildrenRequest = {
  path: string
  showHidden: boolean
}

export type ListTreeChildrenResponse = {
  path: string
  children: TreeChildEntry[]
}

export type VolumeInfo = {
  mountRoot: string
  label: string
  totalBytes: number
  freeBytes: number
  isNetwork: boolean
  isRemovable: boolean
}

export type VolumeCategory = 'fixed' | 'removable' | 'network'

export type VolumeGroup = {
  category: VolumeCategory
  label: string
  volumes: VolumeInfo[]
}
