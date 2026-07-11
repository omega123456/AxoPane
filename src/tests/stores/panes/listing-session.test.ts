import { describe, expect, it, vi } from 'vitest'
import type { DirectoryEntry, SessionBaseline } from '@/lib/types/ipc'
import { ipc } from '@/tests/ipc-mock'
import {
  ListingSession,
  MAX_IN_FLIGHT_REQUESTS,
  prefetchPageIndexes,
} from '@/stores/panes/listing-session'
import { SESSION_PAGE_SIZE } from '@/stores/panes/entry-collection'

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

function baseline(overrides: Partial<SessionBaseline> = {}): SessionBaseline {
  return { sessionId: 1, navigationRevision: 1, watchRevision: 0, viewRevision: 0, ...overrides }
}

function view() {
  return {
    sortKey: 'name' as const,
    sortDirection: 'asc' as const,
    filter: '',
    showHidden: false,
    includeItemCounts: false,
  }
}

function noopCallbacks() {
  return { onChange: vi.fn() }
}

/** A promise the test can resolve/reject on demand, for controlling in-flight timing precisely. */
function deferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((res, rej) => {
    resolve = res
    reject = rej
  })
  return { promise, resolve, reject }
}

/**
 * Drains every in-flight and queued `getDirectorySessionRange` request for
 * `session` by yielding microtasks until nothing is left outstanding (bounded
 * so a genuine bug that never settles fails the test instead of hanging it).
 */
async function flushListingSession(session: ListingSession) {
  for (let iteration = 0; iteration < 200; iteration += 1) {
    if (session.inFlightCount === 0) {
      return
    }
    await Promise.resolve()
  }
  throw new Error('flushListingSession: requests never drained')
}

describe('ListingSession', () => {
  it('begin() installs the first page and total from begin_directory_session', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })

    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    expect(session.currentStatus).toBe('ready')
    expect(session.collection.totalRows).toBe(SESSION_PAGE_SIZE * 3)
    expect(session.collection.isPageLoaded(0)).toBe(true)
    expect(session.currentBaseline).toEqual(baseline())
  })

  it('accepts only a revision-matching session patch and invalidates rows atomically', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: 1,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: [entry(0)] },
    })
    ipc.override('get_directory_session_range', {
      baseline: baseline({ watchRevision: 1, viewRevision: 1 }),
      totalRows: 2,
      page: { pageIndex: 0, entries: [entry(0), entry(1)] },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    expect(
      session.applyPatch({
        mode: 'replaceView',
        paneId: 'left',
        tabId: 'left-1',
        path: 'C:\\root',
        previousBaseline: baseline({ sessionId: 99 }),
        nextBaseline: baseline({ watchRevision: 1, viewRevision: 1 }),
        totalRows: 2,
      }),
    ).toBe(false)
    expect(session.collection.entryAt(0)).toEqual(entry(0))

    expect(
      session.applyPatch({
        mode: 'replaceView',
        paneId: 'left',
        tabId: 'left-1',
        path: 'C:\\root',
        previousBaseline: baseline(),
        nextBaseline: baseline({ watchRevision: 1, viewRevision: 1 }),
        totalRows: 2,
      }),
    ).toBe(true)
    await flushListingSession(session)
    expect(session.currentBaseline).toEqual(baseline({ watchRevision: 1, viewRevision: 1 }))
    expect(session.collection.entryAt(1)).toEqual(entry(1))
  })

  it('permits at most two range requests in flight per pane under rapid scroll-simulated page requests', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    const pending: Array<ReturnType<typeof deferred>> = []
    ipc.override('get_directory_session_range', () => {
      const next = deferred<unknown>()
      pending.push(next)
      return next.promise as never
    })

    // Simulate rapid scrolling requesting five distinct pages back-to-back
    // before any of them has resolved.
    for (const pageIndex of [3, 4, 5, 6, 7]) {
      session.ensurePageLoaded(pageIndex)
    }

    expect(session.inFlightCount).toBeLessThanOrEqual(MAX_IN_FLIGHT_REQUESTS)
    expect(pending.length).toBe(MAX_IN_FLIGHT_REQUESTS)

    // Resolving one in-flight request must let a queued page start, but the
    // bound must hold at every step, never spiking above the limit.
    pending[0].resolve({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      page: { pageIndex: 3, entries: pageEntries(3) },
    })
    await Promise.resolve()
    await Promise.resolve()

    expect(session.inFlightCount).toBeLessThanOrEqual(MAX_IN_FLIGHT_REQUESTS)
    expect(session.collection.isPageLoaded(3)).toBe(true)
  })

  it('forward/backward scrolling refetches an evicted page by index and installs it correctly', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 40,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('get_directory_session_range', (payload) => ({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 40,
      page: { pageIndex: payload.pageIndex, entries: pageEntries(payload.pageIndex) },
    }))

    session.ensurePageLoaded(2)
    await flushListingSession(session)
    expect(session.collection.isPageLoaded(2)).toBe(true)

    // Scroll far away across 20 distinct pages (well past the 12-page cap),
    // repinning the viewport at each step so page 2 is never re-pinned.
    for (let pageIndex = 10; pageIndex < 10 + 20; pageIndex += 1) {
      session.ensurePageLoaded(pageIndex)
      await flushListingSession(session)
    }
    expect(session.collection.isPageLoaded(2)).toBe(false)

    // Scroll back to page 2: it must refetch and install correctly.
    session.ensurePageLoaded(2)
    await flushListingSession(session)

    expect(session.collection.isPageLoaded(2)).toBe(true)
    expect(session.collection.entryAt(2 * SESSION_PAGE_SIZE)?.id).toBe(
      `entry-${2 * SESSION_PAGE_SIZE}`,
    )
  })

  it('discards an in-flight range response that resolves after a newer navigation has already superseded it', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    const staleRequest = deferred<unknown>()
    ipc.override('get_directory_session_range', () => staleRequest.promise as never)
    session.ensurePageLoaded(2)
    expect(session.inFlightCount).toBe(1)

    // A newer navigation begins on the same pane *before* the in-flight range
    // request resolves — this establishes a new (higher) session id.
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\other',
      baseline: baseline({ sessionId: 2, navigationRevision: 2 }),
      totalRows: SESSION_PAGE_SIZE,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0, 3) },
    })
    await session.begin('left-1', 'C:\\other', view())
    expect(session.currentBaseline).toEqual(baseline({ sessionId: 2, navigationRevision: 2 }))

    // Now the superseded page-2 request from the OLD session resolves. It
    // must not be installed into the collection under the new baseline.
    staleRequest.resolve({
      baseline: baseline(), // old baseline (session 1)
      totalRows: SESSION_PAGE_SIZE * 5,
      page: { pageIndex: 2, entries: pageEntries(2) },
    })
    await Promise.resolve()
    await Promise.resolve()

    expect(session.collection.isPageLoaded(2)).toBe(false)
    expect(session.collection.totalRows).toBe(SESSION_PAGE_SIZE)
  })

  it('enters a terminal error state and notifies its owner after a non-protocol range failure', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 2,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const callbacks = noopCallbacks()
    const session = new ListingSession('left', 'left-1', callbacks)
    await session.begin('left-1', 'C:\\root', view())

    ipc.override(
      'get_directory_session_range',
      () => Promise.reject(new Error('transport lost')) as never,
    )
    session.ensurePageLoaded(1)
    await flushListingSession(session)

    expect(session.currentStatus).toBe('error')
    expect(session.currentError).toBe('transport lost')
    expect(callbacks.onChange).toHaveBeenCalled()
  })

  it('a view revision change (reviseView) invalidates every sparse page and cannot be filled by a stale in-flight response', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    const staleRequest = deferred<unknown>()
    ipc.override('get_directory_session_range', () => staleRequest.promise as never)
    session.ensurePageLoaded(2)

    ipc.override('revise_directory_session_view', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline({ viewRevision: 1 }),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    await session.reviseView({ ...view(), sortKey: 'size' })
    expect(session.currentBaseline).toEqual(baseline({ viewRevision: 1 }))

    staleRequest.resolve({
      baseline: baseline(), // pre-revision baseline
      totalRows: SESSION_PAGE_SIZE * 5,
      page: { pageIndex: 2, entries: pageEntries(2) },
    })
    await Promise.resolve()
    await Promise.resolve()

    expect(session.collection.isPageLoaded(2)).toBe(false)
  })

  it('a stale reviseView response cannot downgrade an already-installed newer same-session view baseline', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    // Two overlapping sort/filter changes (e.g. a rapid sort click followed
    // by a filter keystroke) each issue their own `reviseView` request for
    // the same session. Simulate their responses landing out of order: the
    // later-issued (higher `viewRevision`) request resolves first.
    const firstRevise = deferred<unknown>()
    const secondRevise = deferred<unknown>()
    let call = 0
    ipc.override('revise_directory_session_view', () => {
      call += 1
      return (call === 1 ? firstRevise.promise : secondRevise.promise) as never
    })

    const firstPromise = session.reviseView({ ...view(), sortKey: 'size' })
    const secondPromise = session.reviseView({ ...view(), filter: 'x' })

    secondRevise.resolve({
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline({ viewRevision: 2 }),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    await secondPromise
    expect(session.currentBaseline).toEqual(baseline({ viewRevision: 2 }))

    // The earlier-issued request's response arrives late, carrying a lower
    // `viewRevision` for the same session. It must not downgrade the
    // already-installed newer baseline.
    firstRevise.resolve({
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline({ viewRevision: 1 }),
      totalRows: SESSION_PAGE_SIZE * 5,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    await firstPromise

    expect(session.currentBaseline).toEqual(baseline({ viewRevision: 2 }))
  })

  it('empty directories produce zero rows, zero pages, and no ensurePageLoaded activity', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\empty',
      baseline: baseline(),
      totalRows: 0,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: [] },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\empty', view())

    expect(session.collection.totalRows).toBe(0)
    expect(session.collection.loadedPageCount).toBe(0)

    const rangeSpy = vi.fn()
    ipc.override('get_directory_session_range', () => {
      rangeSpy()
      return { baseline: baseline(), totalRows: 0, page: { pageIndex: 0, entries: [] } }
    })
    session.ensurePageLoaded(0)
    expect(rangeSpy).not.toHaveBeenCalled()
  })

  it('release() is idempotent and safe to call twice or with no active session', async () => {
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    // No active session yet: release must be a safe no-op.
    await session.release()

    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0, 3) },
    })
    await session.begin('left-1', 'C:\\root', view())

    const releaseSpy = vi.fn().mockReturnValue({ released: true })
    ipc.override('release_directory_session', releaseSpy)

    await session.release()
    expect(releaseSpy).toHaveBeenCalledTimes(1)
    expect(session.currentStatus).toBe('idle')
    expect(session.collection.totalRows).toBe(0)

    // Calling release again with no active baseline must not re-invoke the
    // command a second time.
    await session.release()
    expect(releaseSpy).toHaveBeenCalledTimes(1)
  })

  it('selection/focus survive page eviction because they are tracked by path/id, not page residence', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 40,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('get_directory_session_range', (payload) => ({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 40,
      page: { pageIndex: payload.pageIndex, entries: pageEntries(payload.pageIndex) },
    }))

    session.ensurePageLoaded(2)
    await flushListingSession(session)
    const selectedPath = 'C:\\root\\file-1000.txt' // row in page 2
    const selectedId = session.collection.findRowIndexByPath(selectedPath)
    expect(selectedId).toBe(1000)

    // Evict page 2 by scrolling far away across 20 distinct pages.
    for (let pageIndex = 10; pageIndex < 10 + 20; pageIndex += 1) {
      session.ensurePageLoaded(pageIndex)
      await flushListingSession(session)
    }
    expect(session.collection.isPageLoaded(2)).toBe(false)

    // The path identity itself is what a selection store should track (not a
    // row index or page reference) — it is simply no longer resident, not
    // corrupted, and refetching brings back the *same* identity at the same
    // row index.
    expect(session.collection.findRowIndexByPath(selectedPath)).toBeUndefined()
    session.ensurePageLoaded(2)
    await flushListingSession(session)
    expect(session.collection.findRowIndexByPath(selectedPath)).toBe(1000)
  })

  it('prefetchPageIndexes pins the viewport plus one adjacent page on each side, clamped to valid range', () => {
    expect(prefetchPageIndexes(5, 20)).toEqual([4, 5, 6])
    expect(prefetchPageIndexes(0, 20)).toEqual([0, 1])
    expect(prefetchPageIndexes(20, 20)).toEqual([19, 20])
  })

  it('begin() surfaces a real transport failure as an error status without throwing', async () => {
    ipc.override('begin_directory_session', () => {
      throw new Error('disk unavailable')
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())

    await session.begin('left-1', 'C:\\root', view())

    expect(session.currentStatus).toBe('error')
    expect(session.currentError).toBe('disk unavailable')
  })

  it('reviseView() surfaces a real transport failure as an error status', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0, 3) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('revise_directory_session_view', () => {
      throw new Error('backend crashed')
    })
    await session.reviseView({ ...view(), sortKey: 'size' })

    expect(session.currentStatus).toBe('error')
    expect(session.currentError).toBe('backend crashed')
  })

  it('reviseView() with no active session is a no-op', async () => {
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.reviseView(view())
    expect(session.currentStatus).toBe('idle')
    expect(session.currentBaseline).toBeNull()
  })

  it('ensurePageLoaded() is a no-op before a session is ready, for an empty view, and for an out-of-range page', async () => {
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    const rangeSpy = vi.fn()
    ipc.override('get_directory_session_range', () => {
      rangeSpy()
      return { baseline: baseline(), totalRows: 0, page: { pageIndex: 0, entries: [] } }
    })

    // No active session yet.
    session.ensurePageLoaded(0)
    expect(rangeSpy).not.toHaveBeenCalled()

    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 2,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    await session.begin('left-1', 'C:\\root', view())

    // Out of range (only pages 0-1 exist).
    session.ensurePageLoaded(5)
    session.ensurePageLoaded(-1)
    expect(rangeSpy).not.toHaveBeenCalled()

    // Already loaded: a second call for page 0 must not re-request it, only
    // bump its recency.
    session.ensurePageLoaded(0)
    expect(rangeSpy).not.toHaveBeenCalled()
  })

  it('a page requested twice while already in flight is not double-dispatched, and the queue skips a page that became loaded before its turn', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    const dispatchSpy = vi.fn()
    const pending: Array<ReturnType<typeof deferred>> = []
    ipc.override('get_directory_session_range', (payload) => {
      dispatchSpy(payload.pageIndex)
      const next = deferred<unknown>()
      pending.push(next)
      return next.promise as never
    })

    // Fill both in-flight slots, then request page 2 again while it is
    // already in flight — must not dispatch a duplicate request.
    session.ensurePageLoaded(2)
    session.ensurePageLoaded(3)
    session.ensurePageLoaded(2)
    expect(dispatchSpy).toHaveBeenCalledTimes(2)
    expect(dispatchSpy.mock.calls.map((call) => call[0]).sort()).toEqual([2, 3])

    // Queue page 4 behind the two in-flight requests.
    session.ensurePageLoaded(4)
    expect(session.inFlightCount).toBe(MAX_IN_FLIGHT_REQUESTS)

    // Resolving page 2 and page 3 frees both in-flight slots, which must
    // dispatch the queued page-4 request.
    pending[0].resolve({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      page: { pageIndex: 2, entries: pageEntries(2) },
    })
    pending[1].resolve({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      page: { pageIndex: 3, entries: pageEntries(3) },
    })
    // Yield enough microtasks for both `.then()` chains to run and for
    // `drainQueue` to dispatch page 4's request (creating `pending[2]`).
    for (let iteration = 0; iteration < 10 && pending.length < 3; iteration += 1) {
      await Promise.resolve()
    }
    expect(pending).toHaveLength(3)
    pending[2].resolve({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 10,
      page: { pageIndex: 4, entries: pageEntries(4) },
    })
    await flushListingSession(session)

    expect(session.collection.isPageLoaded(2)).toBe(true)
    expect(session.collection.isPageLoaded(3)).toBe(true)
    expect(session.collection.isPageLoaded(4)).toBe(true)
    expect(dispatchSpy).toHaveBeenCalledTimes(3)
  })

  it('loadedEntries() returns the underlying collection snapshot for full-listing consumers', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0, 3) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    expect(session.loadedEntries().map((item) => item.id)).toEqual([
      'entry-0',
      'entry-1',
      'entry-2',
    ])
  })

  it('release() logs but does not throw when the underlying command rejects', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0, 3) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('release_directory_session', () => {
      throw new Error('teardown race')
    })

    await expect(session.release()).resolves.toBeUndefined()
    expect(session.currentStatus).toBe('idle')
  })

  it('ignores a stale begin failure after a newer tab navigation becomes ready', async () => {
    const firstBegin = deferred<unknown>()
    const secondBegin = deferred<unknown>()
    const callbacks = noopCallbacks()
    ipc.override('begin_directory_session', ({ tabId }) =>
      tabId === 'left-1' ? (firstBegin.promise as never) : (secondBegin.promise as never),
    )
    const session = new ListingSession('left', 'left-1', callbacks)

    const first = session.begin('left-1', 'C:\\old', view())
    const second = session.begin('left-2', 'C:\\new', view())
    secondBegin.resolve({
      paneId: 'left',
      tabId: 'left-2',
      path: 'C:\\new',
      baseline: baseline({ sessionId: 2 }),
      totalRows: 1,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: [entry(0)] },
    })
    await second
    const changesAfterCurrentBegin = callbacks.onChange.mock.calls.length

    firstBegin.reject(new Error('old tab unavailable'))
    await first

    expect(session.currentStatus).toBe('ready')
    expect(session.currentBaseline).toEqual(baseline({ sessionId: 2 }))
    expect(callbacks.onChange).toHaveBeenCalledTimes(changesAfterCurrentBegin)
  })

  it('does not let stale range completion clear current page bookkeeping after a tab switch', async () => {
    ipc.override('begin_directory_session', ({ tabId }) => ({
      paneId: 'left',
      tabId,
      path: tabId === 'left-1' ? 'C:\\old' : 'C:\\new',
      baseline: baseline({ sessionId: tabId === 'left-1' ? 1 : 2 }),
      totalRows: SESSION_PAGE_SIZE * 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    }))
    const oldRange = deferred<unknown>()
    const newRange = deferred<unknown>()
    let rangeCalls = 0
    ipc.override('get_directory_session_range', () => {
      rangeCalls += 1
      return (rangeCalls === 1 ? oldRange.promise : newRange.promise) as never
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\old', view())
    session.ensurePageLoaded(1)
    expect(session.inFlightCount).toBe(1)

    await session.begin('left-2', 'C:\\new', view())
    session.ensurePageLoaded(1)
    expect(session.inFlightCount).toBe(1)

    oldRange.resolve({
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 3,
      page: { pageIndex: 1, entries: pageEntries(1) },
    })
    await Promise.resolve()
    expect(session.inFlightCount).toBe(1)

    newRange.resolve({
      baseline: baseline({ sessionId: 2 }),
      totalRows: SESSION_PAGE_SIZE * 3,
      page: { pageIndex: 1, entries: pageEntries(1) },
    })
    await flushListingSession(session)
    expect(session.inFlightCount).toBe(0)
    expect(session.collection.isPageLoaded(1)).toBe(true)
  })

  it('a range request rejected with a typed SessionRejection is discarded without surfacing as an error', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('get_directory_session_range', () => {
      throw { kind: 'staleView' }
    })
    session.ensurePageLoaded(1)
    await flushListingSession(session)

    expect(session.currentStatus).toBe('ready')
    expect(session.currentError).toBeNull()
    expect(session.collection.isPageLoaded(1)).toBe(false)
  })

  it('a range request rejected with a real transport error is logged and still drains the queue', async () => {
    ipc.override('begin_directory_session', {
      paneId: 'left',
      tabId: 'left-1',
      path: 'C:\\root',
      baseline: baseline(),
      totalRows: SESSION_PAGE_SIZE * 3,
      pageSize: SESSION_PAGE_SIZE,
      firstPage: { pageIndex: 0, entries: pageEntries(0) },
    })
    const session = new ListingSession('left', 'left-1', noopCallbacks())
    await session.begin('left-1', 'C:\\root', view())

    ipc.override('get_directory_session_range', () => {
      throw new Error('connection reset')
    })
    session.ensurePageLoaded(1)
    await flushListingSession(session)

    expect(session.collection.isPageLoaded(1)).toBe(false)
    expect(session.inFlightCount).toBe(0)
  })
})
