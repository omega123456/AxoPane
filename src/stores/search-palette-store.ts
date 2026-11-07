import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { folderSizeIndexStatus, searchClose, searchPage, searchQuery } from '@/lib/ipc/commands'
import type { SearchCapabilities, SearchHit, SearchRejection, SearchScope } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'

export const SEARCH_DEBOUNCE_MS = 100
export const MIN_SEARCH_CHARS = 3

/** `idle`: nothing to search yet (favourites and recent folders show). */
export type SearchStatus = 'idle' | 'loading' | 'ready' | 'error'

type SearchPaletteState = {
  isOpen: boolean
  /** Bumped by every `open`, so the palette refocuses its input. */
  focusRequest: number
  query: string
  scope: SearchScope
  status: SearchStatus
  queryId: number | null
  items: SearchHit[]
  total: number
  truncated: boolean
  loadingMore: boolean
  activeIndex: number
  error: SearchRejection | null
  capabilities: SearchCapabilities | null
  open: () => void
  close: () => void
  setQuery: (query: string) => void
  setScope: (scope: SearchScope) => void
  setActiveIndex: (index: number) => void
  /** Moves through the results, loading the next page past the last row. */
  moveActive: (delta: number) => void
  loadMore: () => Promise<void>
  reset: () => void
}

const cleared = {
  status: 'idle' as SearchStatus,
  queryId: null,
  items: [],
  total: 0,
  truncated: false,
  loadingMore: false,
  activeIndex: 0,
  error: null,
}

let debounceTimer: ReturnType<typeof setTimeout> | undefined
// Replies to anything but the latest request are dropped.
let latestRequestSequence = 0

/**
 * Mirrors the Rust rule: three non-space characters, or an anchored listing
 * such as `~/`, `/` or `C:\`.
 */
export function isSearchable(query: string) {
  const trimmed = query.trim()
  if (trimmed.replace(/\s/g, '').length >= MIN_SEARCH_CHARS) {
    return true
  }
  return /[\\/]$/.test(trimmed) && /^(~|[\\/]|[A-Za-z]:)/.test(trimmed)
}

function asRejection(error: unknown): SearchRejection {
  if (typeof error === 'object' && error !== null && 'kind' in error) {
    return error as SearchRejection
  }
  return { kind: 'failed', message: String(error) }
}

function cancelPending() {
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer)
    debounceTimer = undefined
  }
  latestRequestSequence += 1
}

export const useSearchPaletteStore = create<SearchPaletteState>((set, get) => {
  async function runQuery() {
    debounceTimer = undefined
    const requestSequence = ++latestRequestSequence
    const { query, scope } = get()
    try {
      const page = await searchQuery({
        query,
        scope,
        includeHidden: usePanesStore.getState().showHiddenFiles,
      })
      if (requestSequence !== latestRequestSequence) {
        return
      }
      set({
        ...cleared,
        status: 'ready',
        queryId: page.queryId,
        items: page.items,
        total: page.total,
        truncated: page.truncated,
        capabilities: page.capabilities,
      })
    } catch (error) {
      const rejection = asRejection(error)
      if (requestSequence !== latestRequestSequence || rejection.kind === 'stale') {
        return
      }
      set(
        rejection.kind === 'tooShort' ? cleared : { ...cleared, status: 'error', error: rejection },
      )
    }
  }

  return {
    isOpen: false,
    focusRequest: 0,
    query: '',
    scope: 'all',
    capabilities: null,
    ...cleared,
    open: () => {
      set((state) => ({ isOpen: true, focusRequest: state.focusRequest + 1 }))
      // Everything's state is pull-only on Windows; refresh it for the palette.
      void folderSizeIndexStatus()
        .then((status) => usePanesStore.getState().setFolderSizeIndexStatus(status))
        .catch((error: unknown) => {
          log.warn('search palette could not refresh the index status', { error })
        })
    },
    close: () => {
      if (!get().isOpen) {
        return
      }
      cancelPending()
      set({ isOpen: false, query: '', ...cleared })
      void searchClose().catch((error: unknown) => {
        log.warn('search close failed', { error })
      })
    },
    setQuery: (query) => {
      cancelPending()
      if (!isSearchable(query)) {
        set({ query, ...cleared })
        return
      }
      // Earlier results stay visible until the new ones arrive.
      set({ query, status: 'loading' })
      debounceTimer = setTimeout(() => void runQuery(), SEARCH_DEBOUNCE_MS)
    },
    setScope: (scope) => {
      set({ scope })
      if (isSearchable(get().query)) {
        cancelPending()
        set({ status: 'loading' })
        void runQuery()
      }
    },
    setActiveIndex: (index) => set({ activeIndex: index }),
    moveActive: (delta) => {
      const { activeIndex, items, total } = get()
      if (items.length === 0) {
        return
      }
      const target = activeIndex + delta
      if (target > items.length - 1 && items.length < total) {
        set({ activeIndex: items.length - 1 })
        void get()
          .loadMore()
          .then(() => set((state) => ({ activeIndex: Math.min(target, state.items.length - 1) })))
        return
      }
      set({ activeIndex: Math.min(Math.max(target, 0), items.length - 1) })
    },
    loadMore: async () => {
      const { queryId, items, total, loadingMore } = get()
      if (queryId === null || loadingMore || items.length >= total) {
        return
      }
      set({ loadingMore: true })
      try {
        const page = await searchPage({ queryId, offset: items.length })
        if (get().queryId !== queryId) {
          return
        }
        set((state) => ({
          items: [...state.items, ...page.items],
          total: page.total,
          loadingMore: false,
        }))
      } catch {
        // A stale page means a newer query is replacing these results.
        if (get().queryId === queryId) {
          set({ loadingMore: false })
        }
      }
    },
    reset: () => {
      cancelPending()
      set({
        isOpen: false,
        focusRequest: 0,
        query: '',
        scope: 'all',
        capabilities: null,
        ...cleared,
      })
    },
  }
})
