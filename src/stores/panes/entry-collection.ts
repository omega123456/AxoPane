/**
 * `PaneEntryCollection`: a sparse, bounded, page-indexed store for one pane's
 * directory rows, backing the v2 range-session contract (Functional
 * Requirement 11 / Design Decision 8 in the perf remediation plan).
 *
 * Contract this module implements:
 *  - At most `MAX_RETAINED_PAGES` (12) 500-row range pages are retained per
 *    collection at once. Installing a 13th unpinned page evicts the least
 *    recently used unpinned page.
 *  - The viewport page and its immediate prefetch neighbors can be pinned so
 *    they are never evicted while still in/near view.
 *  - `totalRows` is tracked independently of how many pages are actually
 *    loaded, so a virtualizer can size scrollbar geometry correctly even when
 *    only a handful of the total pages have ever been fetched.
 *  - `entryAt(rowIndex)` is O(1) (a `Map<pageIndex, DirectoryEntry[]>` slice
 *    lookup), not a linear scan, and returns `undefined` for an unloaded row
 *    (never `null` — `null` is reserved for a row known to be out of range).
 *  - `findRowIndexByPath` / `findRowIndexById` are backed by a `Map` index
 *    over currently loaded rows, not a scan over every loaded entry.
 *  - Appending page N copies only that page's rows (`page.length` more array
 *    slots) — never `[...oldArray, ...newPage]` over the whole collection.
 *
 * This module owns no IPC calls and no React state; `listing-session.ts`
 * drives it by calling `installPage`/`invalidateAll`/`setTotalRows`.
 */

import type { DirectoryEntry } from '@/lib/types/ipc'
import { pathKey } from '@/lib/path-compare'

/** Matches the Rust `SESSION_PAGE_SIZE` constant (`directory_session::model`). */
export const SESSION_PAGE_SIZE = 500

/** At most this many range pages are retained per pane at once. */
export const MAX_RETAINED_PAGES = 12

export function pageIndexForRow(rowIndex: number): number {
  return Math.floor(rowIndex / SESSION_PAGE_SIZE)
}

function pageStartRow(pageIndex: number): number {
  return pageIndex * SESSION_PAGE_SIZE
}

type PageRecord = {
  entries: DirectoryEntry[]
  /** Monotonic recency counter; higher is more recently used. */
  lastAccessedAt: number
}

export class PaneEntryCollection {
  private pages = new Map<number, PageRecord>()
  private pinnedPages = new Set<number>()
  private byPath = new Map<string, number>()
  private byId = new Map<string, number>()
  private accessCounter = 0
  private total = 0

  get totalRows(): number {
    return this.total
  }

  get loadedPageCount(): number {
    return this.pages.size
  }

  get loadedRowCount(): number {
    let count = 0
    for (const page of this.pages.values()) {
      count += page.entries.length
    }
    return count
  }

  isPageLoaded(pageIndex: number): boolean {
    return this.pages.has(pageIndex)
  }

  /**
   * Sets the total-row geometry for the current view. Rows beyond `total`
   * that were previously loaded (e.g. after a shrinking view revision) are
   * dropped; a `total` of `0` clears every page (Phase 4 acceptance: "empty
   * directories render with no phantom/fallback ranges, zero rows, zero
   * pages").
   */
  setTotalRows(total: number): void {
    this.total = Math.max(0, total)
    if (this.total === 0) {
      this.clear()
      return
    }

    const lastValidPage = pageIndexForRow(this.total - 1)
    for (const pageIndex of [...this.pages.keys()]) {
      if (pageIndex > lastValidPage) {
        this.evictPage(pageIndex)
      }
    }
  }

  /**
   * Installs a fetched page, replacing any prior content at that index.
   * Pinned/prefetch-adjacent pages are supplied via `pin` so the eviction
   * pass below never removes the page that was just requested for the
   * current viewport.
   */
  installPage(pageIndex: number, entries: DirectoryEntry[]): void {
    this.evictPageIndexFromIndexes(pageIndex)

    this.accessCounter += 1
    this.pages.set(pageIndex, { entries, lastAccessedAt: this.accessCounter })

    const startRow = pageStartRow(pageIndex)
    entries.forEach((entry, offset) => {
      const rowIndex = startRow + offset
      this.byPath.set(pathKey(entry.path), rowIndex)
      this.byId.set(entry.id, rowIndex)
    })

    this.enforceRetentionLimit()
  }

  /** Marks `pageIndexes` as pinned (never evicted) and everything else as evictable. */
  setPinnedPages(pageIndexes: Iterable<number>): void {
    this.pinnedPages = new Set(pageIndexes)
  }

  /** Touches a page's recency without changing its content (LRU bump on view). */
  markAccessed(pageIndex: number): void {
    const page = this.pages.get(pageIndex)
    if (!page) {
      return
    }
    this.accessCounter += 1
    page.lastAccessedAt = this.accessCounter
  }

  /**
   * Row at `rowIndex`, or `undefined` if that row's page has not been loaded
   * (a legitimate "not yet fetched" sparse slot — distinct from a row past
   * `totalRows`, which callers should check separately via `totalRows`).
   */
  entryAt(rowIndex: number): DirectoryEntry | undefined {
    if (rowIndex < 0 || rowIndex >= this.total) {
      return undefined
    }
    const page = this.pages.get(pageIndexForRow(rowIndex))
    if (!page) {
      return undefined
    }
    return page.entries[rowIndex - pageStartRow(pageIndexForRow(rowIndex))]
  }

  /** All entries currently resident for `pageIndex`, or `undefined` if unloaded. */
  pageEntries(pageIndex: number): DirectoryEntry[] | undefined {
    return this.pages.get(pageIndex)?.entries
  }

  /** O(1) row lookup for a loaded path (independent of page residence). */
  findRowIndexByPath(path: string): number | undefined {
    return this.byPath.get(pathKey(path))
  }

  /** O(1) row lookup for a loaded entry id (independent of page residence). */
  findRowIndexById(id: string): number | undefined {
    return this.byId.get(id)
  }

  /**
   * Replaces one loaded row without rebuilding resident pages. Count events
   * use this to update their visible directory row in O(1); the Rust session
   * remains responsible for any Items-sort reposition and publishes that as
   * a revisioned patch/range instead of renderer-side sorting.
   */
  replaceLoadedEntryByPath(path: string, entry: DirectoryEntry): boolean {
    const rowIndex = this.byPath.get(pathKey(path))
    if (rowIndex === undefined) return false
    const pageIndex = pageIndexForRow(rowIndex)
    const page = this.pages.get(pageIndex)
    if (!page) return false
    const offset = rowIndex - pageStartRow(pageIndex)
    const previous = page.entries[offset]
    if (!previous) return false
    page.entries[offset] = entry
    if (pathKey(previous.path) !== pathKey(entry.path)) {
      this.byPath.delete(pathKey(previous.path))
      this.byPath.set(pathKey(entry.path), rowIndex)
    }
    if (previous.id !== entry.id) {
      this.byId.delete(previous.id)
      this.byId.set(entry.id, rowIndex)
    }
    this.markAccessed(pageIndex)
    return true
  }

  /** Every currently loaded entry, across all resident pages (for consumers that still need a full snapshot, e.g. an entry-count summary). Not the hot rendering path. */
  loadedEntries(): DirectoryEntry[] {
    const result: DirectoryEntry[] = []
    for (const pageIndex of [...this.pages.keys()].sort((left, right) => left - right)) {
      result.push(...this.pages.get(pageIndex)!.entries)
    }
    return result
  }

  /** Which page indexes are currently resident, ascending. */
  loadedPageIndexes(): number[] {
    return [...this.pages.keys()].sort((left, right) => left - right)
  }

  /**
   * Drops every loaded page (used when a view revision changes: sparse
   * unloaded ranges must be invalidated so a stale page index can never be
   * confused with fresh content — Functional Requirement 1 / Phase 4
   * acceptance). `totalRows` and pin state are left to the caller to reset
   * via `setTotalRows`/`setPinnedPages` immediately after.
   */
  invalidateAll(): void {
    this.pages.clear()
    this.byPath.clear()
    this.byId.clear()
    this.pinnedPages.clear()
  }

  private clear(): void {
    this.invalidateAll()
    this.total = 0
  }

  private evictPageIndexFromIndexes(pageIndex: number): void {
    const existing = this.pages.get(pageIndex)
    if (!existing) {
      return
    }
    const startRow = pageStartRow(pageIndex)
    existing.entries.forEach((entry, offset) => {
      const rowIndex = startRow + offset
      if (this.byPath.get(pathKey(entry.path)) === rowIndex) {
        this.byPath.delete(pathKey(entry.path))
      }
      if (this.byId.get(entry.id) === rowIndex) {
        this.byId.delete(entry.id)
      }
    })
    this.pages.delete(pageIndex)
  }

  private evictPage(pageIndex: number): void {
    this.evictPageIndexFromIndexes(pageIndex)
  }

  /**
   * Evicts least-recently-used unpinned pages until at most
   * `MAX_RETAINED_PAGES` remain. If every loaded page is pinned (more than
   * `MAX_RETAINED_PAGES` pinned pages, which should not happen in practice
   * since only the viewport + adjacent prefetch are pinned), retention
   * degrades gracefully by simply not evicting a pinned page rather than
   * evicting to below the limit.
   */
  private enforceRetentionLimit(): void {
    while (this.pages.size > MAX_RETAINED_PAGES) {
      let lruPageIndex: number | undefined
      let lruAccessedAt = Number.POSITIVE_INFINITY
      for (const [pageIndex, record] of this.pages) {
        if (this.pinnedPages.has(pageIndex)) {
          continue
        }
        if (record.lastAccessedAt < lruAccessedAt) {
          lruAccessedAt = record.lastAccessedAt
          lruPageIndex = pageIndex
        }
      }

      if (lruPageIndex === undefined) {
        // Every remaining loaded page is pinned; nothing safe to evict.
        break
      }

      this.evictPage(lruPageIndex)
    }
  }
}
