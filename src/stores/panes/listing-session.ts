import {
  beginDirectorySession,
  getDirectorySessionRange,
  releaseDirectorySession,
  reviseDirectorySessionView,
} from '@/lib/ipc/commands'
import { log } from '@/lib/app-log-commands'
import { errorMessage, isRejectionWithKind } from '@/lib/error-message'
import type {
  BeginNavigationResponse,
  DirectoryEntry,
  SessionBaseline,
  SessionRejection,
  SessionPatchEvent,
  SessionRangeResponse,
  SessionViewParams,
} from '@/lib/types/ipc'
import { MAX_RETAINED_PAGES, PaneEntryCollection, pageIndexForRow } from './entry-collection'

export const MAX_IN_FLIGHT_REQUESTS = 2

export type ListingSessionStatus = 'idle' | 'initializing' | 'ready' | 'error' | 'releasing'

function isSessionRejection(error: unknown): error is SessionRejection {
  return isRejectionWithKind(error) && typeof error.kind === 'string'
}

function baselinesMatch(left: SessionBaseline, right: SessionBaseline): boolean {
  return (
    left.sessionId === right.sessionId &&
    left.navigationRevision === right.navigationRevision &&
    left.watchRevision === right.watchRevision &&
    left.viewRevision === right.viewRevision
  )
}

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

function isPageIndexInRange(pageIndex: number, lastPageIndex: number) {
  return pageIndex >= 0 && pageIndex <= lastPageIndex
}

export function prefetchPageIndexes(viewportPageIndex: number, lastPageIndex: number): number[] {
  const pages = new Set<number>()
  for (let offset = -1; offset <= 1; offset += 1) {
    const candidate = viewportPageIndex + offset
    if (isPageIndexInRange(candidate, lastPageIndex)) {
      pages.add(candidate)
    }
  }
  return [...pages].sort((left, right) => left - right)
}

type SessionLifecycle = {
  status: ListingSessionStatus
  error: string | null
}

function lifecycleIn(status: ListingSessionStatus): SessionLifecycle {
  return { status, error: null }
}

function failedLifecycle(error: string): SessionLifecycle {
  return { status: 'error', error }
}

type ActiveSession = {
  baseline: SessionBaseline
  resolvedPath: string
}

type PendingPages = {
  inFlight: Set<number>
  queued: number[]
}

function emptyPendingPages(): PendingPages {
  return { inFlight: new Set(), queued: [] }
}

export type ListingSessionCallbacks = {
  onChange: () => void
}

export class ListingSession {
  readonly paneId: string
  readonly collection = new PaneEntryCollection()

  private tabId: string
  private active: ActiveSession | null = null
  private lifecycle: SessionLifecycle = lifecycleIn('idle')
  private pending: PendingPages = emptyPendingPages()
  private callbacks: ListingSessionCallbacks
  private lifecycleToken = 0

  constructor(paneId: string, tabId: string, callbacks: ListingSessionCallbacks) {
    this.paneId = paneId
    this.tabId = tabId
    this.callbacks = callbacks
  }

  get currentStatus(): ListingSessionStatus {
    return this.lifecycle.status
  }

  get currentError(): string | null {
    return this.lifecycle.error
  }

  get currentBaseline(): SessionBaseline | null {
    return this.active?.baseline ?? null
  }

  get currentPath(): string | null {
    return this.active?.resolvedPath ?? null
  }

  get inFlightCount(): number {
    return this.pending.inFlight.size
  }

  private discardSessionState() {
    this.active = null
    this.pending = emptyPendingPages()
    this.collection.invalidateAll()
    this.collection.setTotalRows(0)
  }

  private nextLifecycleToken(): number {
    this.lifecycleToken += 1
    return this.lifecycleToken
  }

  private isCurrentLifecycle(token: number): boolean {
    return this.lifecycleToken === token
  }

  async begin(tabId: string, path: string, view: SessionViewParams): Promise<void> {
    const lifecycleToken = this.nextLifecycleToken()
    this.tabId = tabId
    this.lifecycle = lifecycleIn('initializing')
    this.discardSessionState()
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
      this.lifecycle = failedLifecycle(error instanceof Error ? error.message : String(error))
      log.error('ListingSession.begin failed', { paneId: this.paneId, path, error })
      this.callbacks.onChange()
    }
  }

  async reviseView(view: SessionViewParams): Promise<void> {
    if (!this.active) {
      return
    }

    const sessionId = this.active.baseline.sessionId
    const tabId = this.tabId
    const lifecycleToken = this.nextLifecycleToken()
    this.pending = emptyPendingPages()

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
        log.debug('ListingSession.reviseView superseded', {
          paneId: this.paneId,
          kind: error.kind,
        })
        return
      }
      this.lifecycle = failedLifecycle(errorMessage(error))
      log.error('ListingSession.reviseView failed', { paneId: this.paneId, error })
      this.callbacks.onChange()
    }
  }

  applyPatch(patch: SessionPatchEvent): boolean {
    const active = this.active
    if (!active) {
      return false
    }
    if (!this.isOwnPatch(patch)) {
      return false
    }
    const required = patch.mode === 'metadataOnly' ? patch.baseline : patch.previousBaseline
    if (!baselinesMatch(active.baseline, required)) {
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
    this.active = { ...active, baseline: patch.nextBaseline }
    this.collection.invalidateAll()
    this.collection.setTotalRows(patch.totalRows)
    if (patch.totalRows > 0) {
      this.pinViewportPages(0)
      this.ensurePageLoaded(0)
    }
    this.callbacks.onChange()
    return true
  }

  private isOwnPatch(patch: SessionPatchEvent) {
    return patch.paneId === this.paneId && patch.tabId === this.tabId
  }

  private isStillAdoptable(tabId: string, lifecycleToken: number) {
    return this.isCurrentLifecycle(lifecycleToken) && tabId === this.tabId
  }

  private isBehindInstalledBaseline(candidate: SessionBaseline) {
    if (!this.active) {
      return false
    }
    return !baselineIsAtLeastAsAdvanced(this.active.baseline, candidate)
  }

  private adoptBeginResponse(
    tabId: string,
    response: BeginNavigationResponse,
    lifecycleToken: number,
  ) {
    if (!this.isStillAdoptable(tabId, lifecycleToken)) {
      return
    }
    if (this.isBehindInstalledBaseline(response.baseline)) {
      return
    }

    this.active = { baseline: response.baseline, resolvedPath: response.path }
    this.lifecycle = lifecycleIn('ready')
    this.collection.invalidateAll()
    this.collection.setTotalRows(response.totalRows)
    if (response.totalRows > 0) {
      this.collection.installPage(response.firstPage.pageIndex, response.firstPage.entries)
      this.pinViewportPages(response.firstPage.pageIndex)
    }
    this.callbacks.onChange()
  }

  pinViewportPages(viewportPageIndex: number): void {
    if (this.collection.totalRows === 0) {
      this.collection.setPinnedPages([])
      return
    }
    const lastPageIndex = pageIndexForRow(this.collection.totalRows - 1)
    this.collection.setPinnedPages(prefetchPageIndexes(viewportPageIndex, lastPageIndex))
  }

  ensurePageLoaded(pageIndex: number): void {
    if (!this.isReadyWithBaseline()) {
      return
    }
    if (this.collection.totalRows === 0) {
      return
    }
    const lastPageIndex = pageIndexForRow(this.collection.totalRows - 1)
    if (!isPageIndexInRange(pageIndex, lastPageIndex)) {
      return
    }

    this.pinViewportPages(pageIndex)

    if (this.collection.isPageLoaded(pageIndex)) {
      this.collection.markAccessed(pageIndex)
      return
    }
    if (this.isPagePending(pageIndex)) {
      return
    }

    if (this.isAtRequestLimit()) {
      this.pending.queued.push(pageIndex)
      return
    }

    this.dispatchRangeRequest(pageIndex)
  }

  private dispatchRangeRequest(pageIndex: number): void {
    if (!this.active) {
      return
    }
    const baseline = this.active.baseline
    const lifecycleToken = this.lifecycleToken
    this.pending.inFlight.add(pageIndex)

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
        this.pending.inFlight.delete(pageIndex)
        this.installRangeResponseIfCurrent(response, pageIndex)
        this.drainQueue()
      })
      .catch((error: unknown) => {
        if (!this.isCurrentLifecycle(lifecycleToken)) {
          return
        }
        this.pending.inFlight.delete(pageIndex)
        this.recordRangeFailure(error, pageIndex)
        this.drainQueue()
      })
  }

  private isCurrentBaseline(candidate: SessionBaseline): boolean {
    return this.active !== null && baselinesMatch(this.active.baseline, candidate)
  }

  private installRangeResponseIfCurrent(response: SessionRangeResponse, pageIndex: number): void {
    if (!this.isCurrentBaseline(response.baseline)) {
      log.debug('ListingSession discarded stale range response', {
        paneId: this.paneId,
        pageIndex,
      })
      return
    }
    this.collection.setTotalRows(response.totalRows)
    this.collection.installPage(response.page.pageIndex, response.page.entries)
    this.callbacks.onChange()
  }

  private recordRangeFailure(error: unknown, pageIndex: number): void {
    if (isSessionRejection(error)) {
      log.debug('ListingSession range request rejected', {
        paneId: this.paneId,
        pageIndex,
        kind: error.kind,
      })
      return
    }
    this.lifecycle = failedLifecycle(errorMessage(error))
    log.error('ListingSession range request failed', {
      paneId: this.paneId,
      pageIndex,
      error,
    })
    this.callbacks.onChange()
  }

  private isReadyWithBaseline() {
    return this.active !== null && this.lifecycle.status === 'ready'
  }

  private isPagePending(pageIndex: number) {
    return this.pending.inFlight.has(pageIndex) || this.pending.queued.includes(pageIndex)
  }

  private isAtRequestLimit() {
    return this.pending.inFlight.size >= MAX_IN_FLIGHT_REQUESTS
  }

  private canDispatchQueuedPage() {
    return !this.isAtRequestLimit() && this.pending.queued.length > 0
  }

  private drainQueue(): void {
    while (this.canDispatchQueuedPage()) {
      const nextPageIndex = this.pending.queued.shift()!
      if (this.collection.isPageLoaded(nextPageIndex)) {
        continue
      }
      this.dispatchRangeRequest(nextPageIndex)
    }
  }

  loadedEntries(): DirectoryEntry[] {
    return this.collection.loadedEntries()
  }

  async release(): Promise<void> {
    const lifecycleToken = this.nextLifecycleToken()
    const baseline = this.currentBaseline
    this.discardSessionState()
    if (!baseline) {
      this.lifecycle = lifecycleIn('idle')
      this.callbacks.onChange()
      return
    }

    this.lifecycle = { ...this.lifecycle, status: 'releasing' }
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
      log.error('ListingSession.release failed', { paneId: this.paneId, error })
    }
    if (this.isCurrentLifecycle(lifecycleToken)) {
      this.lifecycle = { ...this.lifecycle, status: 'idle' }
      this.callbacks.onChange()
    }
  }
}

export { MAX_RETAINED_PAGES }
