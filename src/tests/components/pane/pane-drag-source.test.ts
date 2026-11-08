import { describe, expect, it } from 'vitest'
import { directoryEntryById, dragItemsForRow } from '@/components/pane/pane-drag-source'
import { directoryEntry, fileEntry } from '@/tests/stores/panes/pane-fixtures'

const ENTRIES = [
  fileEntry('/a'),
  fileEntry('/b'),
  fileEntry('/trashed', { trashId: 't1' }),
  directoryEntry('/dir'),
]

describe('pane drag source', () => {
  it('drags the whole selection when the grabbed row is selected', () => {
    const items = dragItemsForRow(ENTRIES, ['/a', '/b', '/trashed'], '/a')
    expect(items.map((item) => item.path)).toEqual(['/a', '/b'])
    expect(items[0]).toEqual({ id: '/a', name: 'a', path: '/a', isDir: false, sizeBytes: 1 })
  })

  it('drags only the grabbed row otherwise and nothing for trash or missing rows', () => {
    expect(dragItemsForRow(ENTRIES, ['/b'], '/a').map((item) => item.path)).toEqual(['/a'])
    expect(dragItemsForRow(ENTRIES, [], '/trashed')).toEqual([])
    expect(dragItemsForRow(ENTRIES, [], '..')).toEqual([])
  })

  it('finds directory rows only', () => {
    expect(directoryEntryById(ENTRIES, '/dir')?.path).toBe('/dir')
    expect(directoryEntryById(ENTRIES, '/a')).toBeUndefined()
  })
})
