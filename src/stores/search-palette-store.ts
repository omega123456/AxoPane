import { create, type StoreApi } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { isRejectionWithKind } from '@/lib/error-message'
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
  if (isRejectionWithKind(error)) {
    return error as SearchRejection
  }
  return { kind: 'failed', message: String(error) }
}

function isSupersededRequest(requestSequence: number) {
  return requestSequence !== latestRequestSequence
}

function shouldIgnoreRejection(requestSequence: number, rejection: SearchRejection) {
  return isSupersededRequest(requestSequence) || rejection.kind === 'stale'
}

function cancelPending() {
  if (debounceTimer !== undefined) {
    clearTimeout(debounceTimer)
    debounceTimer = undefined
  }
  latestRequestSequence += 1
}

type PaletteSet = StoreApi<SearchPaletteState>['setState']
type PaletteGet = () => SearchPaletteState

async function runQuery(set: PaletteSet, get: PaletteGet) {
  debounceTimer = undefined
  const requestSequence = ++latestRequestSequence
  const { query, scope } = get()
  try {
    const page = await searchQuery({
      query,
      scope,
      includeHidden: usePanesStore.getState().showHiddenFiles,
    })
    if (isSupersededRequest(requestSequence)) {
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
    if (shouldIgnoreRejection(requestSequence, rejection)) {
      return
    }
    set(rejection.kind === 'tooShort' ? cleared : { ...cleared, status: 'error', error: rejection })
  }
}

function openPalette(set: PaletteSet) {
  set((state) => ({ isOpen: true, focusRequest: state.focusRequest + 1 }))
  // Everything's state is pull-only on Windows; refresh it for the palette.
  void folderSizeIndexStatus()
    .then((status) => usePanesStore.getState().setFolderSizeIndexStatus(status))
    .catch((error: unknown) => {
      log.warn('search palette could not refresh the index status', { error })
    })
}

function closePalette(set: PaletteSet, get: PaletteGet) {
  if (!get().isOpen) {
    return
  }
  cancelPending()
  set({ isOpen: false, query: '', ...cleared })
  void searchClose().catch((error: unknown) => {
    log.warn('search close failed', { error })
  })
}

function isPastLoadedButMoreAvailable(target: number, loadedCount: number, total: number) {
  const pastLastLoaded = target > loadedCount - 1
  return pastLastLoaded && loadedCount < total
}

function hasMoreToLoad(state: SearchPaletteState) {
  return !state.loadingMore && state.items.length < state.total
}

function moveActive(set: PaletteSet, get: PaletteGet, delta: number) {
  const { activeIndex, items, total } = get()
  if (items.length === 0) {
    return
  }
  const target = activeIndex + delta
  if (isPastLoadedButMoreAvailable(target, items.length, total)) {
    set({ activeIndex: items.length - 1 })
    void get()
      .loadMore()
      .then(() => set((state) => ({ activeIndex: Math.min(target, state.items.length - 1) })))
    return
  }
  set({ activeIndex: Math.min(Math.max(target, 0), items.length - 1) })
}

async function loadMore(set: PaletteSet, get: PaletteGet) {
  const { queryId, items } = get()
  if (queryId === null) {
    return
  }
  if (!hasMoreToLoad(get())) {
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
}

const INITIAL_PALETTE_STATE = {
  isOpen: false,
  focusRequest: 0,
  query: '',
  scope: 'all',
  capabilities: null,
  ...cleared,
} satisfies Partial<SearchPaletteState>

export const useSearchPaletteStore = create<SearchPaletteState>((set, get) => ({
  ...INITIAL_PALETTE_STATE,
  open: () => openPalette(set),
  close: () => closePalette(set, get),
  setQuery: (query) => {
    cancelPending()
    if (!isSearchable(query)) {
      set({ query, ...cleared })
      return
    }
    // Earlier results stay visible until the new ones arrive.
    set({ query, status: 'loading' })
    debounceTimer = setTimeout(() => void runQuery(set, get), SEARCH_DEBOUNCE_MS)
  },
  setScope: (scope) => {
    set({ scope })
    if (isSearchable(get().query)) {
      cancelPending()
      set({ status: 'loading' })
      void runQuery(set, get)
    }
  },
  setActiveIndex: (index) => set({ activeIndex: index }),
  moveActive: (delta) => moveActive(set, get, delta),
  loadMore: () => loadMore(set, get),
  reset: () => {
    cancelPending()
    set(INITIAL_PALETTE_STATE)
  },
}))
