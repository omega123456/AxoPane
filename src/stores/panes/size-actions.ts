import {
  cancelSizes,
  requestFolderSize,
  requestFolderSizes,
  setSizeSubscriptions,
} from '@/lib/ipc/commands'
import { log } from '@/lib/app-log-commands'
import { isTrashPath } from '@/lib/trash'
import type { FolderSizeIndexStatus, SizeStateEvent } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { panesStore } from '@/stores/panes/store-handle'
import { listingSessions } from '@/stores/panes/listing-runtime'
import { directoryPaths } from '@/stores/panes/pane-state'
import {
  AUTO_FOLDER_SIZE_MAX_DIRECTORIES,
  appliedSizeStates,
  collectPendingSizeRequests,
  eagerSizesEnabled,
  refreshSizePaths,
  staleSizePathsOnLeave,
  summarizeSizeEvents,
  withoutStaleSizes,
  withPendingSizeRequests,
  type SizeEventBatchSummary,
} from '@/stores/panes/size-requests'
import type { PaneId } from '@/types/pane'
import { runMarkingPending } from '@/stores/panes/pending-request'

const STALE_SIZE_LOG_SAMPLE_LIMIT = 3
const SIZE_EVENT_LOG_INTERVAL = 2000

let sizeEventsProcessed = 0

function logSizeEventProgress({
  batchSize,
  calculatingCount,
  terminalCount,
}: SizeEventBatchSummary) {
  const previousBucket = Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL)
  sizeEventsProcessed += batchSize
  const crossedLogInterval =
    Math.floor(sizeEventsProcessed / SIZE_EVENT_LOG_INTERVAL) > previousBucket
  if (!crossedLogInterval) {
    return
  }
  log.debug('size: events applied', {
    totalProcessed: sizeEventsProcessed,
    lastBatch: batchSize,
    calculatingInBatch: calculatingCount,
    terminalInBatch: terminalCount,
  })
}

export async function reportSizeSubscriptions(
  paneId: PaneId,
  currentPath: string | null,
  folderPaths: string[],
) {
  try {
    await setSizeSubscriptions({ paneId, currentPath, folderPaths })
  } catch (error) {
    log.warn('size: setSizeSubscriptions failed', { paneId, error })
  }
}

function cancelStaleSizes(paneId: PaneId, staleSizePaths: string[]) {
  panesStore().setState((state) => withoutStaleSizes(state, staleSizePaths))
  const cancelStartedAt = performance.now()
  void cancelSizes(staleSizePaths)
    .then((response) => {
      log.debug('size: navigate cancel result', {
        paneId,
        requested: staleSizePaths.length,
        cancelled: response.cancelled,
        elapsedMilliseconds: Math.round(performance.now() - cancelStartedAt),
      })
    })
    .catch((error) => {
      log.error('navigatePane failed to cancel folder sizes', { paneId, error })
    })
}

export function releaseSizesOnLeave(paneId: PaneId, destinationPath: string) {
  const state = panesStore().getState()
  const leaving = state.panes[paneId]
  const staleSizePaths = staleSizePathsOnLeave(state, paneId)
  log.debug('size: navigate cancel evaluation', {
    paneId,
    from: leaving.path,
    to: destinationPath,
    leavingDirs: directoryPaths(leaving.entries).length,
    stale: staleSizePaths.length,
    sampleStale: staleSizePaths.slice(0, STALE_SIZE_LOG_SAMPLE_LIMIT),
  })
  if (staleSizePaths.length > 0) {
    cancelStaleSizes(paneId, staleSizePaths)
  }
}

async function requestSizesMarkingPending(paths: string[], request: () => Promise<unknown>) {
  await runMarkingPending({
    markPending: (isPending) =>
      panesStore().setState((state) => withPendingSizeRequests(state, paths, isPending)),
    request,
  })
}

function eagerSizingEnabledNow() {
  return eagerSizesEnabled(
    panesStore().getState().folderSizeIndexStatus,
    useConfigStore.getState().autoFolderSize,
  )
}

export async function requestEagerDirectorySizes(paths: string[]) {
  if (!eagerSizingEnabledNow()) {
    return []
  }
  if (paths.length > AUTO_FOLDER_SIZE_MAX_DIRECTORIES) {
    return []
  }

  const pending = collectPendingSizeRequests(paths, panesStore().getState())
  if (pending.length === 0) {
    return []
  }
  await requestSizesMarkingPending(pending, () =>
    requestFolderSizes({ paths: pending, mode: 'auto' }),
  )
  return pending
}

export async function refreshSizes(paneId: PaneId) {
  await listingSessions[paneId].release()
  await panesStore().getState().reloadPane(paneId)

  const pane = panesStore().getState().panes[paneId]
  if (isTrashPath(pane.path)) {
    return
  }
  const folders = directoryPaths(pane.entries)
  const paths = refreshSizePaths(
    folders,
    panesStore().getState().sizeStates,
    eagerSizingEnabledNow(),
  )
  if (paths.length === 0) {
    return
  }
  await requestSizesMarkingPending(paths, () => requestFolderSizes({ paths, mode: 'refresh' }))
}

export function applySizeStates(events: SizeStateEvent[]) {
  if (events.length === 0) {
    return
  }
  logSizeEventProgress(summarizeSizeEvents(events))
  panesStore().setState((state) => appliedSizeStates(state, events))
}

export async function requestManualSize(paneId: PaneId, entryId: string) {
  const entry = panesStore()
    .getState()
    .panes[paneId].entries.find((item) => item.id === entryId)
  if (!entry?.isDir) {
    return
  }
  await requestFolderSize({ path: entry.path })
}

export async function calculateAllFolderSizes(paneId: PaneId) {
  const pane = panesStore().getState().panes[paneId]
  const paths = directoryPaths(pane.entries)
  const pending = paths.filter((path) => !panesStore().getState().pendingSizeRequests[path])
  log.debug('size: calculateAllFolderSizes', {
    paneId,
    path: pane.path,
    directories: paths.length,
    pending: pending.length,
  })
  if (pending.length === 0) {
    return
  }

  const startedAt = performance.now()
  await requestSizesMarkingPending(pending, () => requestFolderSizes({ paths: pending }))
  log.debug('size: calculateAllFolderSizes request returned', {
    paneId,
    requested: pending.length,
    elapsedMilliseconds: Math.round(performance.now() - startedAt),
  })
}

export function setFolderSizeIndexStatus(folderSizeIndexStatus: FolderSizeIndexStatus) {
  panesStore().setState({ folderSizeIndexStatus })
}
