/**
 * `ListingSession`: per-pane orchestration for the v2 seekable directory
 * session (Functional Requirement 11 / Design Decision 8 / Phase 4).
 *
 * Owns:
 *  - Issuing `getDirectorySessionRange` page requests against a
 *    `PaneEntryCollection`, admitting at most `MAX_IN_FLIGHT_REQUESTS` (2)
 *    concurrent range requests per pane.
 *  - Discarding stale responses: a response is only installed into the
 *    collection if its `baseline` still exactly matches the session's
 *    current baseline at the time the response arrives (not at the time the
 *    request was issued) — a view/navigation change that happened while the
 *    request was in flight makes the response a no-op.
 *  - Explicit release orchestration: `release()` calls
 *    `releaseDirectorySession` for the current baseline and is safe to call
 *    more than once (idempotent on the Rust side too).
 *
 * Does not touch React state directly — `panes-store.ts` drives one
 * `ListingSession` per pane and mirrors its `PaneEntryCollection` into
 * whatever store shape the UI reads. This keeps the sparse-collection
 * mechanics testable in complete isolation from Zustand/React.
 */

import {
  beginDirectorySession,
  getDirectorySessionRange,
  releaseDirectorySession,
  reviseDirectorySessionView,
} from '@/lib/ipc/commands'
import { log } from '@/lib/app-log-commands'
import type {
  BeginNavigationResponse,
  DirectoryEntry,
  SessionBaseline,
  SessionRejection,
  SessionPatchEvent,
  SessionViewParams,
} from '@/lib/types/ipc'
import { MAX_RETAINED_PAGES, PaneEntryCollection, pageIndexForRow } from './entry-collection'

/** At most this many `getDirectorySessionRange` requests may be in flight per pane at once. */
export const MAX_IN_FLIGHT_REQUESTS = 2

export type ListingSessionStatus = 'idle' | 'initializing' | 'ready' | 'error' | 'releasing'

function isSessionRejection(error: unknown): error is SessionRejection {
  return (
    typeof error === 'object' &&
    error !== null &&
    'kind' in error &&
    typeof (error as { kind: unknown }).kind === 'string'
  )
}

function baselinesMatch(left: SessionBaseline, right: SessionBaseline): boolean {
  return (
    left.sessionId === right.sessionId &&
    left.navigationRevision === right.navigationRevision &&
    left.watchRevision === right.watchRevision &&
    left.viewRevision === right.viewRevision
  )
}

/**
 * Whether `candidate` represents the same or later lifecycle state than
 * `current`. A strictly newer `sessionId` always wins outright (a new
 * navigation supersedes any prior session's view/watch state entirely).
 * For the *same* session, `reviseView` keeps `sessionId` fixed but bumps
 * `viewRevision` (and `begin`/watch reconciliation can bump
 * `navigationRevision`/`watchRevision`); two overlapping `reviseView` calls
 * (e.g. a rapid sort change followed by a filter change) can have their
 * responses arrive out of order, so adopting a same-session response must
 * reject one whose revisions are behind what is already installed rather
 * than only comparing `sessionId`.
 */
function baselineIsAtLeastAsAdvanced(
  current: SessionBaseline,
  candidate: SessionBaseline,
): boolean {
  if (candidate.sessionId !== current.sessionId) {
    return candidate.sessionId > current.sessionId
  }
  return (
    candidate.navigationRevision >= current.navigationRevision &&
    candidate.watchRevision >= current.watchRevision &&
    candidate.viewRevision >= current.viewRevision
  )
}

/**
 * Pages to keep pinned (never evicted) for a given viewport page: the
 * viewport itself plus one page of prefetch on either side, matching the
 * plan's "pins the viewport plus adjacent prefetch pages" retention rule.
 */
export function prefetchPageIndexes(viewportPageIndex: number, lastPageIndex: number): number[] {
  const pages = new Set<number>()
  for (let offset = -1; offset <= 1; offset += 1) {
    const candidate = viewportPageIndex + offset
    if (candidate >= 0 && candidate <= lastPageIndex) {
      pages.add(candidate)
    }
  }
  return [...pages].sort((left, right) => left - right)
}

export type ListingSessionCallbacks = {
  /** Called after any state change that should cause the owning store to re-derive its public pane snapshot. */
  onChange: () => void
}

export class ListingSession {
  readonly paneId: string
  readonly collection = new PaneEntryCollection()

  private tabId: string
  private baseline: SessionBaseline | null = null
  private view: SessionViewParams | null = null
  private status: ListingSessionStatus = 'idle'
  private error: string | null = null
  private inFlightPages = new Set<number>()
  private queuedPages: number[] = []
  private callbacks: ListingSessionCallbacks
  /**
   * Fences every asynchronous lifecycle operation. A late begin/release/range
   * completion belongs to the token that created it and must not alter a
   * newer navigation's status, page bookkeeping, or notifications.
   */
  private lifecycleToken = 0

  constructor(paneId: string, tabId: string, callbacks: ListingSessionCallbacks) {
    this.paneId = paneId
    this.tabId = tabId
    this.callbacks = callbacks
  }

  get currentStatus(): ListingSessionStatus {
    return this.status
  }

  get currentError(): string | null {
    return this.error
  }

  get currentBaseline(): SessionBaseline | null {
    return this.baseline
  }

  get currentPath(): string | null {
    return this.resolvedPath
  }

  get inFlightCount(): number {
    return this.inFlightPages.size
  }

  private resolvedPath: string | null = null

  private nextLifecycleToken(): number {
    this.lifecycleToken += 1
    return this.lifecycleToken
  }

  private isCurrentLifecycle(token: number): boolean {
    return this.lifecycleToken === token
  }

  /**
   * Begins a brand-new session for `path` on this pane/tab, replacing any
   * previously active session (the Rust side already retires the prior
   * session for the pane inside `begin_navigation`; this call additionally
   * discards this session's own local sparse state and in-flight bookkeeping
   * so a stale response from the old session can never be installed).
   */
  async begin(tabId: string, path: string, view: SessionViewParams): Promise<void> {
    const lifecycleToken = this.nextLifecycleToken()
    this.tabId = tabId
    this.view = view
    this.status = 'initializing'
    this.error = null
    // A brand-new navigation invalidates every locally cached page and every
    // in-flight/queued request from the previous session immediately — even
    // before the network round trip resolves — so nothing further can be
    // installed under the old identity.
    this.baseline = null
    this.resolvedPath = null
    this.collection.invalidateAll()
    this.collection.setTotalRows(0)
    this.inFlightPages.clear()
    this.queuedPages = []
    this.callbacks.onChange()

    try {
      const response = await beginDirectorySession({
        paneId: this.paneId,
        tabId,
        path,
        view,
      })
      this.adoptBeginResponse(tabId, response, lifecycleToken)
    } catch (error) {
      if (!this.isCurrentLifecycle(lifecycleToken)) {
        return
      }
      this.status = 'error'
      this.error = error instanceof Error ? error.message : String(error)
      log.error('ListingSession.begin failed', { paneId: this.paneId, path, error })
      this.callbacks.onChange()
    }
  }

  /**
   * Revises the view (sort/filter/show-hidden) for the already-active
   * session without a fresh `begin_directory_session` round trip. Every
   * sparse page is invalidated (the Rust view revision has changed, so any
   * unloaded/loaded page index may now contain entirely different rows).
   */
  async reviseView(view: SessionViewParams): Promise<void> {
    if (!this.baseline) {
      // No active session yet; nothing to revise — callers should `begin` first.
      return
    }

    const sessionId = this.baseline.sessionId
    const tabId = this.tabId
    const lifecycleToken = this.nextLifecycleToken()
    this.view = view
    this.inFlightPages.clear()
    this.queuedPages = []

    try {
      const response = await reviseDirectorySessionView({
        paneId: this.paneId,
        tabId,
        sessionId,
        view,
      })
      this.adoptBeginResponse(tabId, response, lifecycleToken)
    } catch (error) {
      if (!this.isCurrentLifecycle(lifecycleToken)) {
        return
      }
      if (isSessionRejection(error)) {
        // The session was superseded (e.g. a newer navigation) while this
        // revise was in flight — silently drop it rather than surfacing an
        // error for a request that is no longer relevant.
        log.debug('ListingSession.reviseView superseded', {
          paneId: this.paneId,
          kind: error.kind,
        })
        return
      }
      this.status = 'error'
      this.error = error instanceof Error ? error.message : String(error)
      log.error('ListingSession.reviseView failed', { paneId: this.paneId, error })
      this.callbacks.onChange()
    }
  }

  /**
   * Accepts only a patch for this exact session/tab baseline.  Patches can
   * reorder rows across page boundaries, so replacing the sparse view and
   * pulling page zero is deliberately preferred to renderer-side reordering.
   */
  applyPatch(patch: SessionPatchEvent): boolean {
    if (patch.paneId !== this.paneId || patch.tabId !== this.tabId || !this.baseline) {
      return false
    }
    const required = patch.mode === 'metadataOnly' ? patch.baseline : patch.previousBaseline
    if (!baselinesMatch(this.baseline, required)) {
      return false
    }

    if (patch.mode === 'metadataOnly') {
      for (const update of patch.updates) {
        this.collection.replaceLoadedEntryByPath(update.path, update.entry)
      }
      this.callbacks.onChange()
      return true
    }

    this.nextLifecycleToken()
    this.baseline = patch.nextBaseline
    this.collection.invalidateAll()
    this.collection.setTotalRows(patch.totalRows)
    if (patch.totalRows > 0) {
      this.pinViewportPages(0)
      this.ensurePageLoaded(0)
    }
    this.callbacks.onChange()
    return true
  }

  private adoptBeginResponse(
    tabId: string,
    response: BeginNavigationResponse,
    lifecycleToken: number,
  ) {
    // A later `begin`/`reviseView` call may have already superseded this one
    // by the time the response arrives (e.g. rapid double-navigation, or two
    // overlapping `reviseView` calls for the same session whose responses
    // arrive out of order); only adopt it if this is still the tab we issued
    // the request for and it is not behind the baseline already installed.
    if (!this.isCurrentLifecycle(lifecycleToken) || tabId !== this.tabId) {
      return
    }
    if (this.baseline && !baselineIsAtLeastAsAdvanced(this.baseline, response.baseline)) {
      return
    }

    this.resolvedPath = response.path
    this.baseline = response.baseline
    this.status = 'ready'
    this.error = null
    this.collection.invalidateAll()
    this.collection.setTotalRows(response.totalRows)
    // An empty view (`totalRows === 0`) must produce zero loaded pages, not a
    // phantom page 0 record with an empty entries array (Phase 4 acceptance:
    // "empty directories render with no phantom/fallback ranges").
    if (response.totalRows > 0) {
      this.collection.installPage(response.firstPage.pageIndex, response.firstPage.entries)
      this.pinViewportPages(response.firstPage.pageIndex)
    }
    this.callbacks.onChange()
  }

  /** Pins the viewport page and its immediate prefetch neighbors so eviction never targets them. */
  pinViewportPages(viewportPageIndex: number): void {
    if (this.collection.totalRows === 0) {
      this.collection.setPinnedPages([])
      return
    }
    const lastPageIndex = pageIndexForRow(this.collection.totalRows - 1)
    this.collection.setPinnedPages(prefetchPageIndexes(viewportPageIndex, lastPageIndex))
  }

  /**
   * Ensures `pageIndex` is loaded (or already in flight / queued), pinning
   * it and its prefetch neighbors first. At most `MAX_IN_FLIGHT_REQUESTS`
   * requests run concurrently for this pane; further requests queue and are
   * drained as in-flight requests complete.
   */
  ensurePageLoaded(pageIndex: number): void {
    if (!this.baseline || this.status !== 'ready') {
      return
    }
    if (this.collection.totalRows === 0) {
      return
    }
    const lastPageIndex = pageIndexForRow(this.collection.totalRows - 1)
    if (pageIndex < 0 || pageIndex > lastPageIndex) {
      return
    }

    this.pinViewportPages(pageIndex)

    if (this.collection.isPageLoaded(pageIndex)) {
      this.collection.markAccessed(pageIndex)
      return
    }
    if (this.inFlightPages.has(pageIndex) || this.queuedPages.includes(pageIndex)) {
      return
    }

    if (this.inFlightPages.size >= MAX_IN_FLIGHT_REQUESTS) {
      this.queuedPages.push(pageIndex)
      return
    }

    this.dispatchRangeRequest(pageIndex)
  }

  private dispatchRangeRequest(pageIndex: number): void {
    const baseline = this.baseline
    if (!baseline) {
      return
    }
    const lifecycleToken = this.lifecycleToken
    this.inFlightPages.add(pageIndex)

    void getDirectorySessionRange({
      paneId: this.paneId,
      tabId: this.tabId,
      baseline,
      pageIndex,
    })
      .then((response) => {
        if (!this.isCurrentLifecycle(lifecycleToken)) {
          return
        }
        this.inFlightPages.delete(pageIndex)
        // The response is only meaningful if this session's baseline has not
        // moved on since the request was issued — a superseded baseline (new
        // navigation, or a view revision from `reviseView`) means this page
        // index may not even mean the same thing anymore under the new view.
        if (!this.baseline || !baselinesMatch(this.baseline, response.baseline)) {
          log.debug('ListingSession discarded stale range response', {
            paneId: this.paneId,
            pageIndex,
          })
        } else {
          this.collection.setTotalRows(response.totalRows)
          this.collection.installPage(response.page.pageIndex, response.page.entries)
          this.callbacks.onChange()
        }
        this.drainQueue()
      })
      .catch((error: unknown) => {
        if (!this.isCurrentLifecycle(lifecycleToken)) {
          return
        }
        this.inFlightPages.delete(pageIndex)
        if (isSessionRejection(error)) {
          // Stale/rejected range fetch: nothing to install, and nothing to
          // surface as a user-facing error — the active navigation/view has
          // already moved on.
          log.debug('ListingSession range request rejected', {
            paneId: this.paneId,
            pageIndex,
            kind: error.kind,
          })
        } else {
          this.status = 'error'
          this.error = error instanceof Error ? error.message : String(error)
          log.error('ListingSession range request failed', {
            paneId: this.paneId,
            pageIndex,
            error,
          })
          // A non-protocol error has no newer baseline to recover us. Notify
          // the pane walk so it can terminate rather than await forever.
          this.callbacks.onChange()
        }
        this.drainQueue()
      })
  }

  private drainQueue(): void {
    while (this.inFlightPages.size < MAX_IN_FLIGHT_REQUESTS && this.queuedPages.length > 0) {
      const nextPageIndex = this.queuedPages.shift()!
      if (this.collection.isPageLoaded(nextPageIndex)) {
        continue
      }
      this.dispatchRangeRequest(nextPageIndex)
    }
  }

  /** All entries currently loaded for the active view, sorted by row index (convenience for full-collection consumers; not the hot rendering path). */
  loadedEntries(): DirectoryEntry[] {
    return this.collection.loadedEntries()
  }

  /**
   * Idempotent release: tells Rust to retire the active session (safe to
   * call when there is no active session, or more than once). Always safe to
   * call during navigation-away, tab close, or app teardown.
   */
  async release(): Promise<void> {
    const lifecycleToken = this.nextLifecycleToken()
    const baseline = this.baseline
    if (!baseline) {
      this.status = 'idle'
      this.error = null
      this.resolvedPath = null
      this.inFlightPages.clear()
      this.queuedPages = []
      this.collection.invalidateAll()
      this.collection.setTotalRows(0)
      this.callbacks.onChange()
      return
    }

    this.status = 'releasing'
    this.baseline = null
    this.inFlightPages.clear()
    this.queuedPages = []
    this.collection.invalidateAll()
    this.collection.setTotalRows(0)
    this.callbacks.onChange()

    try {
      await releaseDirectorySession({
        paneId: this.paneId,
        tabId: this.tabId,
        sessionId: baseline.sessionId,
        navigationRevision: baseline.navigationRevision,
      })
    } catch (error) {
      if (!this.isCurrentLifecycle(lifecycleToken)) {
        return
      }
      // Release is best-effort: teardown/navigation-away must not throw.
      log.error('ListingSession.release failed', { paneId: this.paneId, error })
    }
    if (this.isCurrentLifecycle(lifecycleToken)) {
      this.status = 'idle'
      this.callbacks.onChange()
    }
  }
}

export { MAX_RETAINED_PAGES }
