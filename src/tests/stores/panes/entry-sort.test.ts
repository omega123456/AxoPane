import { describe, expect, it } from 'vitest'
import {
  clientSortedEntries,
  insertSorted,
  nextSortDirection,
  sortedPanes,
  sortEntries,
  sortSpecFor,
  trashSortedPanes,
} from '@/stores/panes/entry-sort'
import { directoryEntry, fileEntry, paneWith, panesDataWithLeft } from './pane-fixtures'

function paths(entries: { path: string }[]) {
  return entries.map((entry) => entry.path)
}

describe('entry sort', () => {
  it('puts folders first and orders names in either direction', () => {
    const entries = [fileEntry('/b10'), fileEntry('/b2'), directoryEntry('/z')]
    expect(
      paths(sortEntries(entries, { sortKey: 'name', sortDirection: 'asc', sizeStates: {} })),
    ).toEqual(['/z', '/b10', '/b2'])
    expect(
      paths(sortEntries(entries, { sortKey: 'name', sortDirection: 'desc', sizeStates: {} })),
    ).toEqual(['/z', '/b2', '/b10'])
  })

  it('keeps unknown sizes after known ones and prefers live size states', () => {
    const entries = [directoryEntry('/unknown'), directoryEntry('/small'), directoryEntry('/big')]
    const sizeStates = {
      '/small': { state: 'ready' as const, sizeBytes: 1, source: 'manual' as const },
      '/big': { state: 'ready' as const, sizeBytes: 9, source: 'manual' as const },
    }
    expect(
      paths(sortEntries(entries, { sortKey: 'size', sortDirection: 'desc', sizeStates })),
    ).toEqual(['/big', '/small', '/unknown'])
  })

  it('orders by modified date and falls back to the name on ties', () => {
    const entries = [
      fileEntry('/b', { modifiedAt: '2024-01-01' }),
      fileEntry('/a', { modifiedAt: '2024-01-01' }),
      fileEntry('/c', { modifiedAt: '2023-01-01' }),
    ]
    expect(
      paths(sortEntries(entries, { sortKey: 'modified', sortDirection: 'asc', sizeStates: {} })),
    ).toEqual(['/c', '/a', '/b'])
  })

  it('binary-inserts into an already sorted listing', () => {
    const entries = [fileEntry('/a'), fileEntry('/c')]
    insertSorted(entries, fileEntry('/b'), {
      sortKey: 'name',
      sortDirection: 'asc',
      sizeStates: {},
    })
    expect(paths(entries)).toEqual(['/a', '/b', '/c'])
  })

  it('only re-sorts materialized entries on the client for the size key', () => {
    const entries = [fileEntry('/b', { sizeBytes: 1 }), fileEntry('/a', { sizeBytes: 5 })]
    const byName = paneWith('left', { sortKey: 'name', sortDirection: 'asc' })
    const bySize = paneWith('left', { sortKey: 'size', sortDirection: 'desc' })
    expect(clientSortedEntries(entries, byName, {})).toBe(entries)
    expect(paths(clientSortedEntries(entries, bySize, {}))).toEqual(['/a', '/b'])
  })

  it('flips the active column and starts new columns in their natural direction', () => {
    const pane = paneWith('left', { sortKey: 'name', sortDirection: 'asc' })
    expect(nextSortDirection(pane, 'name')).toBe('desc')
    expect(nextSortDirection(pane, 'type')).toBe('asc')
    expect(nextSortDirection(pane, 'size')).toBe('desc')
  })

  it('sorts trash panes entirely on the client', () => {
    const state = panesDataWithLeft({ entries: [fileEntry('/a'), fileEntry('/b')] })
    const next = trashSortedPanes(state, 'left', 'name').panes.left
    expect(next.sortDirection).toBe('desc')
    expect(paths(next.entries)).toEqual(['/b', '/a'])
    expect(next.itemsSortStatus).toBe('complete')
  })

  it('keeps the rows and starts counting for an items sort', () => {
    const entries = [fileEntry('/b'), fileEntry('/a')]
    const state = panesDataWithLeft({ entries })
    const itemsSorted = sortedPanes(state, 'left', 'items').panes.left
    expect(itemsSorted.entries).toBe(entries)
    expect(itemsSorted.itemsSortStatus).toBe('counting')
    const nameSorted = sortedPanes(state, 'left', 'type').panes.left
    expect(nameSorted.itemsSortStatus).toBe('idle')
    expect(nameSorted.sortDirection).toBe('asc')
  })

  it('builds a sort spec from the pane order and the live size states', () => {
    const sizeStates = {}
    const pane = paneWith('left', { sortKey: 'size', sortDirection: 'desc' })
    const spec = sortSpecFor(pane, sizeStates)
    expect(spec).toMatchObject({ sortKey: 'size', sortDirection: 'desc' })
    expect(spec.sizeStates).toBe(sizeStates)
  })
})
