import type { DirectoryEntry } from '@/lib/types/ipc'
import { pathKey } from '@/lib/path-compare'

// Must equal Rust `SESSION_PAGE_SIZE` in `directory_session::model`.
export const SESSION_PAGE_SIZE = 500

export const MAX_RETAINED_PAGES = 12

export function pageIndexForRow(rowIndex: number): number {
  return Math.floor(rowIndex / SESSION_PAGE_SIZE)
}

function pageStartRow(pageIndex: number): number {
  return pageIndex * SESSION_PAGE_SIZE
}

type PageRecord = {
  entries: DirectoryEntry[]
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

  private isRowInRange(rowIndex: number) {
    return rowIndex >= 0 && rowIndex < this.total
  }

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

  setPinnedPages(pageIndexes: Iterable<number>): void {
    this.pinnedPages = new Set(pageIndexes)
  }

  markAccessed(pageIndex: number): void {
    const page = this.pages.get(pageIndex)
    if (!page) {
      return
    }
    this.accessCounter += 1
    page.lastAccessedAt = this.accessCounter
  }

  entryAt(rowIndex: number): DirectoryEntry | undefined {
    if (!this.isRowInRange(rowIndex)) {
      return undefined
    }
    const page = this.pages.get(pageIndexForRow(rowIndex))
    if (!page) {
      return undefined
    }
    return page.entries[rowIndex - pageStartRow(pageIndexForRow(rowIndex))]
  }

  pageEntries(pageIndex: number): DirectoryEntry[] | undefined {
    return this.pages.get(pageIndex)?.entries
  }

  findRowIndexByPath(path: string): number | undefined {
    return this.byPath.get(pathKey(path))
  }

  findRowIndexById(id: string): number | undefined {
    return this.byId.get(id)
  }

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
    const pathChanged = pathKey(previous.path) !== pathKey(entry.path)
    if (pathChanged) {
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

  loadedEntries(): DirectoryEntry[] {
    const result: DirectoryEntry[] = []
    for (const pageIndex of [...this.pages.keys()].sort((left, right) => left - right)) {
      result.push(...this.pages.get(pageIndex)!.entries)
    }
    return result
  }

  loadedPageIndexes(): number[] {
    return [...this.pages.keys()].sort((left, right) => left - right)
  }

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
      const entryPathKey = pathKey(entry.path)
      const pathIndexedHere = this.byPath.get(entryPathKey) === rowIndex
      if (pathIndexedHere) {
        this.byPath.delete(entryPathKey)
      }
      const idIndexedHere = this.byId.get(entry.id) === rowIndex
      if (idIndexedHere) {
        this.byId.delete(entry.id)
      }
    })
    this.pages.delete(pageIndex)
  }

  private evictPage(pageIndex: number): void {
    this.evictPageIndexFromIndexes(pageIndex)
  }

  private enforceRetentionLimit(): void {
    while (this.pages.size > MAX_RETAINED_PAGES) {
      const lruPageIndex = this.leastRecentlyUsedUnpinnedPage()
      if (lruPageIndex === undefined) {
        break
      }
      this.evictPage(lruPageIndex)
    }
  }

  private leastRecentlyUsedUnpinnedPage(): number | undefined {
    let lruPageIndex: number | undefined
    let lruAccessedAt = Number.POSITIVE_INFINITY
    for (const [pageIndex, record] of this.pages) {
      const olderUnpinned =
        !this.pinnedPages.has(pageIndex) && record.lastAccessedAt < lruAccessedAt
      if (olderUnpinned) {
        lruAccessedAt = record.lastAccessedAt
        lruPageIndex = pageIndex
      }
    }
    return lruPageIndex
  }
}
