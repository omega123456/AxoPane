import type { DirectoryEntry, SortDirection, SortKey } from '@/lib/types/ipc'

export type PaneId = 'left' | 'right'

export type PaneState = {
  id: PaneId
  title: string
  path: string
  entries: DirectoryEntry[]
  focusedEntryId: string | null
  sortKey: SortKey
  sortDirection: SortDirection
  filterDraft: string
  filterApplied: string
  typing: boolean
  loading: boolean
  error: string | null
  visibleStartIndex: number
  visibleEndIndex: number
}
