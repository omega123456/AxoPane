import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { fixtures } from '@/tests/fixtures'
import type { SearchHit, SearchPage, SearchQueryRequest } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import {
  SEARCH_DEBOUNCE_MS,
  isSearchable,
  useSearchPaletteStore,
} from '@/stores/search-palette-store'

function hit(index: number): SearchHit {
  return {
    path: `C:\\Work\\report-${index}.docx`,
    name: `report-${index}.docx`,
    parentPath: 'C:\\Work',
    isDir: false,
    modifiedAt: null,
    nameRanges: [[0, 6]],
    parentRanges: [],
  }
}

function page(queryId: number, items: SearchHit[], total = items.length, offset = 0): SearchPage {
  return {
    queryId,
    offset,
    total,
    truncated: false,
    items,
    capabilities: { backend: 'everything', foldersOnly: false },
  }
}

const store = () => useSearchPaletteStore.getState()

async function typeAndSettle(query: string) {
  store().setQuery(query)
  await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS)
}

beforeEach(() => {
  vi.useFakeTimers()
  usePanesStore.getState().reset()
  store().reset()
})

afterEach(() => {
  store().reset()
  vi.useRealTimers()
})

describe('isSearchable', () => {
  it('needs three non-space characters unless the query lists an anchored folder', () => {
    expect(isSearchable('ab')).toBe(false)
    expect(isSearchable(' a b ')).toBe(false)
    expect(isSearchable('abc')).toBe(true)
    expect(isSearchable('~/')).toBe(true)
    expect(isSearchable('/')).toBe(true)
    expect(isSearchable('C:\\')).toBe(true)
    expect(isSearchable('a/')).toBe(false)
  })
})

describe('search palette store', () => {
  it('opens with a focus request and refreshes the index status', async () => {
    ipc.override('folder_size_index_status', {
      backend: 'macIndex',
      isAvailable: true,
      state: 'indexing',
      itemsScanned: 10,
    })
    store().open()
    expect(store().isOpen).toBe(true)
    expect(store().focusRequest).toBe(1)
    await vi.runAllTimersAsync()
    expect(usePanesStore.getState().folderSizeIndexStatus?.state).toBe('indexing')

    ipc.override('folder_size_index_status', () => Promise.reject(new Error('gone')))
    store().open()
    await vi.runAllTimersAsync()
    expect(store().focusRequest).toBe(2)
  })

  it('debounces queries and sends the scope and hidden-files setting', async () => {
    const requests: SearchQueryRequest[] = []
    ipc.override('search_query', (request) => {
      requests.push(request)
      return page(7, [hit(1)])
    })
    usePanesStore.setState({ showHiddenFiles: true })
    store().setQuery('rep')
    expect(store().status).toBe('loading')
    store().setQuery('repo')
    await vi.advanceTimersByTimeAsync(SEARCH_DEBOUNCE_MS)
    expect(requests).toEqual([{ query: 'repo', scope: 'all', includeHidden: true }])
    expect(store()).toMatchObject({ status: 'ready', queryId: 7, total: 1, activeIndex: 0 })
    expect(store().capabilities?.backend).toBe('everything')
  })

  it('clears results for a short query', async () => {
    await typeAndSettle('report')
    store().setQuery('re')
    expect(store()).toMatchObject({ status: 'idle', items: [], queryId: null, query: 're' })
  })

  it('drops replies to superseded queries', async () => {
    let releaseFirst: (value: SearchPage) => void = () => {}
    ipc.override('search_query', (request) =>
      request.query === 'first'
        ? new Promise<SearchPage>((resolve) => {
            releaseFirst = resolve
          })
        : page(2, [hit(2)]),
    )
    await typeAndSettle('first')
    await typeAndSettle('second')
    releaseFirst(page(1, [hit(1)]))
    await vi.runAllTimersAsync()
    expect(store().queryId).toBe(2)
    expect(store().items).toEqual([hit(2)])
  })

  it('maps rejections to states', async () => {
    ipc.override('search_query', () => Promise.reject({ kind: 'stale' }))
    await typeAndSettle('stale one')
    expect(store().status).toBe('loading')

    ipc.override('search_query', () => Promise.reject({ kind: 'tooShort' }))
    await typeAndSettle('C:/')
    expect(store().status).toBe('idle')

    ipc.override('search_query', () => Promise.reject({ kind: 'notReady' }))
    await typeAndSettle('report')
    expect(store()).toMatchObject({ status: 'error', error: { kind: 'notReady' } })

    ipc.override('search_query', () => Promise.reject(new Error('boom')))
    await typeAndSettle('reports')
    expect(store().error).toEqual({ kind: 'failed', message: 'Error: boom' })
  })

  it('reruns at once when the scope changes', async () => {
    const scopes: string[] = []
    ipc.override('search_query', (request) => {
      scopes.push(request.scope)
      return page(3, [hit(1)])
    })
    store().setScope('folders')
    expect(scopes).toEqual([])
    await typeAndSettle('report')
    store().setScope('files')
    await vi.runAllTimersAsync()
    expect(scopes).toEqual(['folders', 'files'])
    expect(store().scope).toBe('files')
  })

  it('moves the highlight and loads the next page past the last row', async () => {
    ipc.override('search_query', page(4, [hit(0), hit(1)], 3))
    ipc.override('search_page', (request) => page(4, [hit(request.offset)], 3, request.offset))
    store().moveActive(1)
    expect(store().activeIndex).toBe(0)
    await typeAndSettle('report')
    store().moveActive(-5)
    expect(store().activeIndex).toBe(0)
    store().moveActive(1)
    expect(store().activeIndex).toBe(1)
    store().moveActive(1)
    await vi.runAllTimersAsync()
    expect(store().items).toHaveLength(3)
    expect(store().activeIndex).toBe(2)
    store().setActiveIndex(0)
    expect(store().activeIndex).toBe(0)
  })

  it('loads pages only when more remain and the query is current', async () => {
    let pages = 0
    ipc.override('search_query', page(5, [hit(0)], 2))
    ipc.override('search_page', () => {
      pages += 1
      return page(5, [hit(1)], 2, 1)
    })
    await store().loadMore()
    expect(pages).toBe(0)
    await typeAndSettle('report')
    await store().loadMore()
    await store().loadMore()
    expect(pages).toBe(1)
    expect(store().items).toHaveLength(2)

    ipc.override('search_query', page(6, [hit(0)], 2))
    ipc.override('search_page', () => {
      useSearchPaletteStore.setState({ queryId: 99 })
      return page(6, [hit(1)], 2, 1)
    })
    await typeAndSettle('reports')
    await store().loadMore()
    expect(store().items).toHaveLength(1)

    ipc.override('search_query', page(8, [hit(0)], 2))
    ipc.override('search_page', () => Promise.reject({ kind: 'stale' }))
    await typeAndSettle('reportz')
    await store().loadMore()
    expect(store().loadingMore).toBe(false)
  })

  it('closes, cancels pending work and tells Rust to drop the results', async () => {
    let closes = 0
    ipc.override('search_close', () => {
      closes += 1
    })
    store().close()
    expect(closes).toBe(0)
    store().open()
    store().setQuery('report')
    store().close()
    await vi.runAllTimersAsync()
    expect(store()).toMatchObject({ isOpen: false, query: '', status: 'idle' })
    expect(closes).toBe(1)

    ipc.override('search_close', () => Promise.reject(new Error('closed')))
    store().open()
    store().close()
    await vi.runAllTimersAsync()
    expect(store().isOpen).toBe(false)
    expect(fixtures.search_close).toBeUndefined()
  })
})
