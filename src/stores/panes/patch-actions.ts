import { log } from '@/lib/app-log-commands'
import type { DirPatchEvent, SessionPatchEvent } from '@/lib/types/ipc'
import { panesStore } from '@/stores/panes/store-handle'
import { dirPatchOutcome, paneIdFromTabId } from '@/stores/panes/dir-patch'
import { runActiveItemsSortForPane } from '@/stores/panes/item-count-actions'
import {
  ensureSessionFullyLoaded,
  listingSessions,
  nextListRequestId,
  replacementViewTokens,
} from '@/stores/panes/listing-runtime'
import type { PaneState } from '@/stores/panes/pane-state'
import {
  dirPatchFromSessionPatch,
  replacementViewApplied,
  sessionPaneId,
  type ReplacementView,
} from '@/stores/panes/session-patch'
import { activeTab, isActiveTabShowingPath } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

export function applyDirPatch(event: DirPatchEvent) {
  const state = panesStore().getState()
  const paneId = paneIdFromTabId(event.tabId)
  const pane = state.panes[paneId]
  const outcome = dirPatchOutcome(state, paneId, event, activeTab(paneId).id)

  if (outcome.kind === 'skipped') {
    logSkippedDirPatch(outcome.reason, paneId, pane, event)
    return
  }
  panesStore().setState(outcome.next)
  if (outcome.kind === 'itemsSortPending') {
    queueMicrotask(() => void runActiveItemsSortForPane(paneId))
    return
  }
  log.debug('applyDirPatch applied', {
    paneId,
    path: event.path,
    entries: outcome.entryCount,
    changed: event.changed.length,
    removed: event.removed.length,
  })
}

const dirPatchSkipLoggers = {
  typing: (paneId: PaneId, _pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (typing)', { paneId, path: event.path }),
  inactiveTab: (paneId: PaneId, _pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (inactive tab)', { paneId, tabId: event.tabId }),
  pathMismatch: (paneId: PaneId, pane: PaneState, event: DirPatchEvent) =>
    log.debug('applyDirPatch skipped (path mismatch)', {
      paneId,
      panePath: pane.path,
      eventPath: event.path,
    }),
}

function logSkippedDirPatch(
  reason: keyof typeof dirPatchSkipLoggers,
  paneId: PaneId,
  pane: PaneState,
  event: DirPatchEvent,
) {
  dirPatchSkipLoggers[reason](paneId, pane, event)
}

function sessionStillShowsEvent(paneId: PaneId, event: SessionPatchEvent) {
  const paneShownPath = panesStore().getState().panes[paneId].path
  return isActiveTabShowingPath(paneId, paneShownPath, event)
}

type ReplaceViewEvent = Extract<SessionPatchEvent, { mode: 'replaceView' }>

async function settledReplacementBaseline(
  paneId: PaneId,
  event: ReplaceViewEvent,
  replacementToken: number,
) {
  const session = listingSessions[paneId]
  let acceptedBaseline = event.nextBaseline
  while (replacementViewTokens[paneId] === replacementToken) {
    await ensureSessionFullyLoaded(paneId, session)
    if (!sessionStillShowsEvent(paneId, event)) {
      return null
    }
    if (session.currentBaseline === acceptedBaseline) {
      return acceptedBaseline
    }
    if (!session.currentBaseline) {
      return null
    }
    acceptedBaseline = session.currentBaseline
  }
  return null
}

async function materializeReplacementView(paneId: PaneId, event: ReplaceViewEvent) {
  replacementViewTokens[paneId] += 1
  const replacementToken = replacementViewTokens[paneId]
  const session = listingSessions[paneId]
  const acceptedBaseline = await settledReplacementBaseline(paneId, event, replacementToken)
  const replacementSuperseded =
    acceptedBaseline === null || replacementViewTokens[paneId] !== replacementToken
  if (replacementSuperseded) {
    return
  }

  const replacement: ReplacementView = {
    paneId,
    tabId: event.tabId,
    path: event.path,
    loadedEntries: session.loadedEntries(),
    requestId: nextListRequestId(),
  }
  const stillApplicable =
    session.currentBaseline === acceptedBaseline && sessionStillShowsEvent(paneId, event)
  if (stillApplicable) {
    panesStore().setState((state) => replacementViewApplied(state, replacement))
  }
  void runActiveItemsSortForPane(paneId)
}

export function applySessionPatch(event: SessionPatchEvent) {
  const paneId = sessionPaneId(event)
  if (!paneId) {
    return
  }
  if (!sessionStillShowsEvent(paneId, event)) {
    return
  }
  const session = listingSessions[paneId]
  if (!session.applyPatch(event)) {
    return
  }

  if (event.mode === 'replaceView') {
    void materializeReplacementView(paneId, event).catch((error) => {
      log.error('applySessionPatch failed to materialize replacement view', {
        paneId,
        path: event.path,
        error,
      })
    })
    return
  }
  panesStore().getState().applyDirPatch(dirPatchFromSessionPatch(event))
}
