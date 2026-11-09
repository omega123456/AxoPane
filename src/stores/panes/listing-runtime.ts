import { ListingSession } from '@/stores/panes/listing-session'
import { pageIndexForRow } from '@/stores/panes/entry-collection'
import type { PaneState } from '@/stores/panes/pane-state'
import type { DirectoryEntry, SessionViewParams } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

export const FIRST_PAGE_INDEX = 0
const FAILED_RANGE_MESSAGE = 'Failed to load directory range'

const pendingRevealPaths: Partial<Record<PaneId, string>> = {}
let revealScrollRequestCounter = 0
let listRequestIdCounter = 0

const sessionChangeWaiters: Record<PaneId, Set<() => void>> = { left: new Set(), right: new Set() }

function notifySessionChange(paneId: PaneId) {
  for (const waiter of sessionChangeWaiters[paneId]) {
    waiter()
  }
}

export const listingSessions: Record<PaneId, ListingSession> = {
  left: new ListingSession('left', 'left', { onChange: () => notifySessionChange('left') }),
  right: new ListingSession('right', 'right', { onChange: () => notifySessionChange('right') }),
}

export const replacementViewTokens: Record<PaneId, number> = { left: 0, right: 0 }

export function nextListRequestId() {
  listRequestIdCounter += 1
  return listRequestIdCounter
}

export function currentViewParams(pane: PaneState, showHiddenFiles: boolean): SessionViewParams {
  return {
    sortKey: pane.sortKey,
    sortDirection: pane.sortDirection,
    filter: pane.filterApplied,
    showHidden: showHiddenFiles,
    includeItemCounts: false,
  }
}

function waitForSessionChange(paneId: PaneId, check: () => boolean): Promise<void> {
  if (check()) {
    return Promise.resolve()
  }
  return new Promise<void>((resolve) => {
    const waiter = () => {
      if (!check()) {
        return
      }
      sessionChangeWaiters[paneId].delete(waiter)
      resolve()
    }
    sessionChangeWaiters[paneId].add(waiter)
  })
}

type SessionPageLoad = {
  pageIndex: number
  stillCurrent: () => boolean
}

async function loadSessionPage(
  paneId: PaneId,
  session: ListingSession,
  { pageIndex, stillCurrent }: SessionPageLoad,
) {
  const { collection } = session
  session.ensurePageLoaded(pageIndex)
  await waitForSessionChange(
    paneId,
    () =>
      !stillCurrent() || collection.isPageLoaded(pageIndex) || session.currentStatus === 'error',
  )
  throwIfSessionFailed(session, FAILED_RANGE_MESSAGE)
}

export function throwIfSessionFailed(session: ListingSession, fallbackMessage: string) {
  if (session.currentStatus === 'error') {
    throw new Error(session.currentError ?? fallbackMessage)
  }
}

export async function ensureSessionFullyLoaded(
  paneId: PaneId,
  session: ListingSession,
): Promise<void> {
  const baselineAtStart = session.currentBaseline
  const { collection } = session
  if (collection.totalRows === 0) {
    return
  }
  if (!baselineAtStart) {
    return
  }

  const stillCurrent = () => session.currentBaseline === baselineAtStart
  const lastPageIndex = pageIndexForRow(collection.totalRows - 1)
  const allPageIndexes = Array.from({ length: lastPageIndex + 1 }, (_, index) => index)
  collection.setPinnedPages(allPageIndexes)

  for (const pageIndex of allPageIndexes) {
    if (!stillCurrent()) {
      return
    }
    if (collection.isPageLoaded(pageIndex)) {
      continue
    }
    await loadSessionPage(paneId, session, { pageIndex, stillCurrent })
  }

  if (stillCurrent()) {
    session.pinViewportPages(FIRST_PAGE_INDEX)
  }
}

const entryIndexByArray = new WeakMap<DirectoryEntry[], Map<string, DirectoryEntry>>()

export function indexEntriesById(entries: DirectoryEntry[]): Map<string, DirectoryEntry> {
  let index = entryIndexByArray.get(entries)
  if (!index) {
    index = new Map(entries.map((entry) => [entry.id, entry]))
    entryIndexByArray.set(entries, index)
  }
  return index
}

export function parkRevealPath(paneId: PaneId, revealTarget: string | undefined) {
  if (revealTarget) {
    pendingRevealPaths[paneId] = revealTarget
    return
  }
  delete pendingRevealPaths[paneId]
}

export function takeRevealPath(paneId: PaneId) {
  const revealPath = pendingRevealPaths[paneId]
  delete pendingRevealPaths[paneId]
  return revealPath
}

export function clearRevealPaths() {
  delete pendingRevealPaths.left
  delete pendingRevealPaths.right
}

export function nextRevealScrollRequestId(revealEntry: DirectoryEntry | undefined) {
  if (revealEntry) {
    revealScrollRequestCounter += 1
  }
  return revealScrollRequestCounter
}
