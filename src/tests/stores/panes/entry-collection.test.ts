import { describe, expect, it } from 'vitest'
import type { DirectoryEntry } from '@/lib/types/ipc'
import {
  MAX_RETAINED_PAGES,
  PaneEntryCollection,
  SESSION_PAGE_SIZE,
  pageIndexForRow,
} from '@/stores/panes/entry-collection'

function entry(index: number): DirectoryEntry {
  return {
    id: `entry-${index}`,
    name: `file-${index}.txt`,
    path: `C:\\root\\file-${index}.txt`,
    isDir: false,
    sizeBytes: 1,
    itemCount: null,
    typeLabel: 'TXT file',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

function pageEntries(pageIndex: number, count = SESSION_PAGE_SIZE): DirectoryEntry[] {
  const start = pageIndex * SESSION_PAGE_SIZE
  return Array.from({ length: count }, (_, offset) => entry(start + offset))
}

describe('PaneEntryCollection', () => {
  it('pageIndexForRow maps row index to the containing 500-row page', () => {
    expect(pageIndexForRow(0)).toBe(0)
    expect(pageIndexForRow(499)).toBe(0)
    expect(pageIndexForRow(500)).toBe(1)
    expect(pageIndexForRow(1999)).toBe(3)
  })

  it('preserves total-row geometry independent of how many pages are loaded', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(50_000)

    expect(collection.totalRows).toBe(50_000)
    expect(collection.loadedPageCount).toBe(0)
    expect(collection.loadedRowCount).toBe(0)
    // A row in an unloaded page is a legitimate sparse slot: undefined, not
    // an error and not a synthesized placeholder entry.
    expect(collection.entryAt(25_000)).toBeUndefined()
  })

  it('installs a page and makes its rows addressable by row index, path, and id', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE)
    collection.installPage(0, pageEntries(0))

    expect(collection.loadedPageCount).toBe(1)
    expect(collection.loadedRowCount).toBe(SESSION_PAGE_SIZE)
    expect(collection.entryAt(0)?.id).toBe('entry-0')
    expect(collection.entryAt(499)?.id).toBe('entry-499')
    expect(collection.findRowIndexByPath('C:\\root\\file-250.txt')).toBe(250)
    expect(collection.findRowIndexById('entry-250')).toBe(250)
  })

  it('entryAt returns undefined for a row past totalRows', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(10)
    collection.installPage(0, pageEntries(0, 10))

    expect(collection.entryAt(9)).toBeDefined()
    expect(collection.entryAt(10)).toBeUndefined()
    expect(collection.entryAt(-1)).toBeUndefined()
  })

  it('appending a page never re-copies earlier pages (O(page size), not O(total loaded))', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE * 3)
    collection.installPage(0, pageEntries(0))
    const page0Entries = collection.pageEntries(0)

    collection.installPage(1, pageEntries(1))

    // Page 0's stored array is the exact same reference after installing
    // page 1 — proof no `[...oldArray, ...newPage]`-style whole-collection
    // copy happened.
    expect(collection.pageEntries(0)).toBe(page0Entries)
    expect(collection.pageEntries(1)).toHaveLength(SESSION_PAGE_SIZE)
  })

  it('retains at most MAX_RETAINED_PAGES (12) pages, evicting least-recently-used unpinned pages', () => {
    const collection = new PaneEntryCollection()
    const totalPages = MAX_RETAINED_PAGES + 5
    collection.setTotalRows(totalPages * SESSION_PAGE_SIZE)

    for (let pageIndex = 0; pageIndex < totalPages; pageIndex += 1) {
      collection.installPage(pageIndex, pageEntries(pageIndex))
      expect(collection.loadedPageCount).toBeLessThanOrEqual(MAX_RETAINED_PAGES)
    }

    expect(collection.loadedPageCount).toBe(MAX_RETAINED_PAGES)
    // The oldest pages (0 and 1) should have been evicted first (pure LRU,
    // nothing pinned in this scenario).
    expect(collection.isPageLoaded(0)).toBe(false)
    expect(collection.isPageLoaded(1)).toBe(false)
    expect(collection.isPageLoaded(totalPages - 1)).toBe(true)
  })

  it('never evicts a pinned page even when it is the least recently used', () => {
    const collection = new PaneEntryCollection()
    const totalPages = MAX_RETAINED_PAGES + 5
    collection.setTotalRows(totalPages * SESSION_PAGE_SIZE)

    // Pin page 0 (as if it were still the viewport) before loading enough
    // pages to otherwise trigger its eviction.
    collection.installPage(0, pageEntries(0))
    collection.setPinnedPages([0])

    for (let pageIndex = 1; pageIndex < totalPages; pageIndex += 1) {
      collection.installPage(pageIndex, pageEntries(pageIndex))
    }

    expect(collection.isPageLoaded(0)).toBe(true)
    // Retention still holds for everything else: pinned page 0 plus the 11
    // most-recently-loaded unpinned pages.
    expect(collection.loadedPageCount).toBe(MAX_RETAINED_PAGES)
  })

  it('a scroll-far-then-back sequence evicts and then correctly re-signals a page as unloaded for refetch', () => {
    const collection = new PaneEntryCollection()
    const totalPages = MAX_RETAINED_PAGES + 3
    collection.setTotalRows(totalPages * SESSION_PAGE_SIZE)

    collection.installPage(2, pageEntries(2))
    expect(collection.isPageLoaded(2)).toBe(true)

    // Scroll far away: load enough new pages to evict page 2 (never pinning it again).
    for (let pageIndex = 5; pageIndex < 5 + MAX_RETAINED_PAGES; pageIndex += 1) {
      collection.installPage(pageIndex, pageEntries(pageIndex))
    }
    expect(collection.isPageLoaded(2)).toBe(false)

    // Scroll back to page 2: the collection correctly reports it unloaded so
    // the caller (listing-session.ts) knows to refetch it by index.
    expect(collection.entryAt(2 * SESSION_PAGE_SIZE)).toBeUndefined()

    // Refetching re-populates it with identical, correctly addressed content.
    collection.installPage(2, pageEntries(2))
    expect(collection.isPageLoaded(2)).toBe(true)
    expect(collection.entryAt(2 * SESSION_PAGE_SIZE)?.id).toBe('entry-1000')
  })

  it('setTotalRows(0) clears every page, matching "empty directories render with zero rows, zero pages"', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE)
    collection.installPage(0, pageEntries(0))
    expect(collection.loadedPageCount).toBe(1)

    collection.setTotalRows(0)

    expect(collection.totalRows).toBe(0)
    expect(collection.loadedPageCount).toBe(0)
    expect(collection.loadedRowCount).toBe(0)
    expect(collection.findRowIndexByPath('C:\\root\\file-0.txt')).toBeUndefined()
  })

  it('shrinking totalRows drops pages entirely beyond the new last valid page', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE * 3)
    collection.installPage(0, pageEntries(0))
    collection.installPage(2, pageEntries(2))

    collection.setTotalRows(SESSION_PAGE_SIZE) // Only page 0 remains valid.

    expect(collection.isPageLoaded(0)).toBe(true)
    expect(collection.isPageLoaded(2)).toBe(false)
  })

  it('invalidateAll drops every page and index without touching totalRows (view-revision invalidation)', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE * 2)
    collection.installPage(0, pageEntries(0))
    collection.installPage(1, pageEntries(1))

    collection.invalidateAll()

    expect(collection.loadedPageCount).toBe(0)
    expect(collection.findRowIndexByPath('C:\\root\\file-0.txt')).toBeUndefined()
    expect(collection.findRowIndexById('entry-0')).toBeUndefined()
    // Geometry (totalRows) is a caller decision on invalidation, not implied
    // by invalidateAll itself — the row count is unchanged until the caller
    // calls setTotalRows again with the new view's total.
    expect(collection.totalRows).toBe(SESSION_PAGE_SIZE * 2)
  })

  it('re-installing a page updates path/id indexes so a moved/renamed entry is found at its new identity only', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE)
    collection.installPage(0, pageEntries(0, 3))

    const renamed: DirectoryEntry = {
      ...entry(1),
      name: 'renamed.txt',
      path: 'C:\\root\\renamed.txt',
    }
    collection.installPage(0, [entry(0), renamed, entry(2)])

    expect(collection.findRowIndexByPath('C:\\root\\file-1.txt')).toBeUndefined()
    expect(collection.findRowIndexByPath('C:\\root\\renamed.txt')).toBe(1)
    // Same id, so the id index still finds it at its (possibly new) row.
    expect(collection.findRowIndexById('entry-1')).toBe(1)
  })

  it('markAccessed on an already-loaded page bumps its recency without changing content', () => {
    const collection = new PaneEntryCollection()
    const totalPages = MAX_RETAINED_PAGES + 2
    collection.setTotalRows(totalPages * SESSION_PAGE_SIZE)

    collection.installPage(0, pageEntries(0))
    for (let pageIndex = 1; pageIndex < MAX_RETAINED_PAGES; pageIndex += 1) {
      collection.installPage(pageIndex, pageEntries(pageIndex))
    }
    // Touch page 0 so it is no longer the least-recently-used page.
    collection.markAccessed(0)

    // Loading two more pages should now evict pages 1 and 2 (the new LRU
    // pages) instead of page 0.
    collection.installPage(MAX_RETAINED_PAGES, pageEntries(MAX_RETAINED_PAGES))
    collection.installPage(MAX_RETAINED_PAGES + 1, pageEntries(MAX_RETAINED_PAGES + 1))

    expect(collection.isPageLoaded(0)).toBe(true)
    expect(collection.isPageLoaded(1)).toBe(false)
  })

  it('loadedEntries returns loaded rows in ascending page order without including sparse gaps', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE * 3)
    collection.installPage(2, pageEntries(2, 2))
    collection.installPage(0, pageEntries(0, 2))

    const loaded = collection.loadedEntries()
    expect(loaded.map((item) => item.id)).toEqual([
      'entry-0',
      'entry-1',
      'entry-1000',
      'entry-1001',
    ])
  })

  it('loadedPageIndexes reports resident pages in ascending order', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE * 5)
    collection.installPage(3, pageEntries(3, 1))
    collection.installPage(1, pageEntries(1, 1))

    expect(collection.loadedPageIndexes()).toEqual([1, 3])
  })

  it('replaces a loaded count row through indexes without rebuilding resident pages', () => {
    const collection = new PaneEntryCollection()
    collection.setTotalRows(SESSION_PAGE_SIZE)
    collection.installPage(0, pageEntries(0, 2))
    const changed = { ...entry(1), itemCount: 42 }

    expect(collection.replaceLoadedEntryByPath(changed.path, changed)).toBe(true)
    expect(collection.entryAt(1)?.itemCount).toBe(42)
    expect(collection.loadedPageCount).toBe(1)
    expect(collection.replaceLoadedEntryByPath('C:\\root\\not-loaded', changed)).toBe(false)
  })
})
