import type { DirectoryEntry, SortDirection, SortKey } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { lookupOwnProperty } from '@/lib/collections'
import { HALF_DIVISOR } from '@/lib/numbers'
import {
  withPane,
  type EntrySizeState,
  type PaneState,
  type PanesData,
  type PanesPatch,
  type SizeStates,
} from '@/stores/panes/pane-state'

export type SortSpec = {
  sortKey: SortKey
  sortDirection: SortDirection
  sizeStates: SizeStates
}

type EntrySortValue = (
  entry: DirectoryEntry,
  sizeState: EntrySizeState | undefined,
) => string | number

const UNKNOWN_NUMBER = -1
const UNKNOWN_DATE = ''
const ASCENDING_ORDER = 1
const DESCENDING_ORDER = -1
const BEFORE = -1
const AFTER = 1
const SAME = 0

const entrySortValues: Record<SortKey, EntrySortValue> = {
  name: (entry) => entry.name.toLowerCase(),
  size: (entry, sizeState) => sizeState?.sizeBytes ?? entry.sizeBytes ?? UNKNOWN_NUMBER,
  items: (entry) => entry.itemCount ?? UNKNOWN_NUMBER,
  type: (entry) => entry.typeLabel.toLowerCase(),
  modified: (entry) => entry.modifiedAt ?? UNKNOWN_DATE,
  created: (entry) => entry.createdAt ?? UNKNOWN_DATE,
}

const ASCENDING_FIRST_SORT_KEYS: ReadonlySet<SortKey> = new Set(['name', 'type'])

const FLIPPED_DIRECTION: Record<SortDirection, SortDirection> = { asc: 'desc', desc: 'asc' }

function entrySortValue(
  entry: DirectoryEntry,
  sizeState: EntrySizeState | undefined,
  sortKey: SortKey,
) {
  const sortValue = lookupOwnProperty(entrySortValues, sortKey) ?? entrySortValues.name
  return sortValue(entry, sizeState)
}

function isKnownSize(entry: DirectoryEntry, sizeState: EntrySizeState | undefined) {
  return (sizeState?.sizeBytes ?? entry.sizeBytes) != null
}

function compareEntryNames(left: DirectoryEntry, right: DirectoryEntry) {
  return left.name.localeCompare(right.name, undefined, {
    numeric: true,
    sensitivity: 'base',
  })
}

function compareSizeKnowledge(
  left: DirectoryEntry,
  right: DirectoryEntry,
  sizeStates: SizeStates,
): number | null {
  const leftHasKnownSize = isKnownSize(left, sizeStates[left.path])
  const rightHasKnownSize = isKnownSize(right, sizeStates[right.path])
  if (leftHasKnownSize !== rightHasKnownSize) {
    return leftHasKnownSize ? BEFORE : AFTER
  }
  return leftHasKnownSize ? null : compareEntryNames(left, right)
}

function compareSortValues(left: string | number, right: string | number) {
  if (left < right) {
    return BEFORE
  }
  return left > right ? AFTER : SAME
}

export function sortSpecFor(
  pane: Pick<PaneState, 'sortKey' | 'sortDirection'>,
  sizeStates: SizeStates,
): SortSpec {
  return { sortKey: pane.sortKey, sortDirection: pane.sortDirection, sizeStates }
}

export function compareEntries(
  left: DirectoryEntry,
  right: DirectoryEntry,
  spec: SortSpec,
): number {
  const { sortKey, sortDirection, sizeStates } = spec
  if (left.isDir !== right.isDir) {
    return left.isDir ? BEFORE : AFTER
  }

  const sizeOrder = sortKey === 'size' ? compareSizeKnowledge(left, right, sizeStates) : null
  if (sizeOrder !== null) {
    return sizeOrder
  }

  const direction = sortDirection === 'asc' ? ASCENDING_ORDER : DESCENDING_ORDER
  const valueOrder = compareSortValues(
    entrySortValue(left, sizeStates[left.path], sortKey),
    entrySortValue(right, sizeStates[right.path], sortKey),
  )
  const order = valueOrder || compareEntryNames(left, right)
  return order * direction
}

export function sortEntries(entries: DirectoryEntry[], spec: SortSpec) {
  return [...entries].sort((left, right) => compareEntries(left, right, spec))
}

export function insertSorted(entries: DirectoryEntry[], entry: DirectoryEntry, spec: SortSpec) {
  let low = 0
  let high = entries.length
  while (low < high) {
    const middle = Math.floor((low + high) / HALF_DIVISOR)
    const sortsBefore = compareEntries(entries[middle], entry, spec) < 0
    if (sortsBefore) {
      low = middle + 1
      continue
    }
    high = middle
  }
  entries.splice(low, 0, entry)
}

export function clientSortedEntries(
  loadedEntries: DirectoryEntry[],
  pane: Pick<PaneState, 'sortKey' | 'sortDirection'>,
  sizeStates: SizeStates,
) {
  if (pane.sortKey !== 'size') {
    return loadedEntries
  }
  return sortEntries(loadedEntries, sortSpecFor(pane, sizeStates))
}

export function nextSortDirection(pane: PaneState, sortKey: SortKey): SortDirection {
  if (pane.sortKey === sortKey) {
    return FLIPPED_DIRECTION[pane.sortDirection]
  }
  return ASCENDING_FIRST_SORT_KEYS.has(sortKey) ? 'asc' : 'desc'
}

type SortableState = Pick<PanesData, 'panes' | 'sizeStates'>

export function trashSortedPanes(
  state: SortableState,
  paneId: PaneId,
  sortKey: SortKey,
): PanesPatch {
  const pane = state.panes[paneId]
  const sortDirection = nextSortDirection(pane, sortKey)
  return withPane(state, paneId, {
    sortKey,
    sortDirection,
    entries: sortEntries(pane.entries, { sortKey, sortDirection, sizeStates: state.sizeStates }),
    itemsSortStatus: 'complete',
  })
}

export function sortedPanes(state: SortableState, paneId: PaneId, sortKey: SortKey): PanesPatch {
  const pane = state.panes[paneId]
  const sortDirection = nextSortDirection(pane, sortKey)
  const isItemsSort = sortKey === 'items'
  return withPane(state, paneId, {
    entries: isItemsSort
      ? pane.entries
      : sortEntries(pane.entries, { sortKey, sortDirection, sizeStates: state.sizeStates }),
    sortKey,
    sortDirection,
    itemsSortStatus: isItemsSort ? 'counting' : 'idle',
  })
}
