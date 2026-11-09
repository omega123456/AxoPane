import type { DirectoryEntry, FolderSizeIndexStatus, SizeStateEvent } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { withKeyFlags } from '@/lib/collections'
import { otherPane } from '@/lib/pane-id'
import { pruneSizeCache } from '@/stores/panes/cache-policy'
import {
  directoryPaths,
  type EntrySizeState,
  type PaneRecord,
  type PanesData,
  type PendingRequests,
  type SizeStateKind,
  type SizeStates,
} from '@/stores/panes/pane-state'

export const AUTO_FOLDER_SIZE_MAX_DIRECTORIES = 500

export type SizeEventBatchSummary = {
  batchSize: number
  calculatingCount: number
  terminalCount: number
}

type SizeRequestState = Pick<PanesData, 'sizeStates' | 'pendingSizeRequests'>

const TERMINAL_SIZE_STATES: ReadonlySet<SizeStateKind> = new Set(['ready', 'na', 'error'])
const NEEDLESS_REQUEST_SIZE_STATES: ReadonlySet<SizeStateKind> = new Set([
  'calculating',
  ...TERMINAL_SIZE_STATES,
])

export function isTerminalSizeState(state: SizeStateKind) {
  return TERMINAL_SIZE_STATES.has(state)
}

export function paneDirectoryCount(entries: DirectoryEntry[]): number {
  return entries.reduce((count, entry) => (entry.isDir ? count + 1 : count), 0)
}

export function autoFolderSizeDisabledForPane(entries: DirectoryEntry[]): boolean {
  return paneDirectoryCount(entries) > AUTO_FOLDER_SIZE_MAX_DIRECTORIES
}

export function eagerSizesEnabled(
  status: FolderSizeIndexStatus | null,
  autoFolderSize: boolean,
): boolean {
  return (status?.isAvailable ?? false) && autoFolderSize
}

function shouldSkipSizeRequest(path: string, state: SizeRequestState) {
  if (state.pendingSizeRequests[path]) {
    return true
  }
  const sizeState = state.sizeStates[path]
  return sizeState !== undefined && NEEDLESS_REQUEST_SIZE_STATES.has(sizeState.state)
}

export function collectPendingSizeRequests(paths: string[], state: SizeRequestState) {
  const unique = Array.from(new Set(paths))
  return unique.filter((path) => !shouldSkipSizeRequest(path, state))
}

export function withPendingSizeRequests(
  state: Pick<PanesData, 'pendingSizeRequests'>,
  paths: string[],
  pending: boolean,
): Pick<PanesData, 'pendingSizeRequests'> {
  return { pendingSizeRequests: withKeyFlags(state.pendingSizeRequests, paths, pending) }
}

function isSizeInFlight(state: SizeRequestState, path: string) {
  const sizeState = state.sizeStates[path]
  return (
    Boolean(state.pendingSizeRequests[path]) ||
    sizeState?.state === 'calculating' ||
    sizeState?.refreshing === true
  )
}

export function staleSizePathsOnLeave(
  state: SizeRequestState & Pick<PanesData, 'panes'>,
  paneId: PaneId,
) {
  const otherDirPaths = new Set(directoryPaths(state.panes[otherPane(paneId)].entries))
  return directoryPaths(state.panes[paneId].entries).filter(
    (path) => !otherDirPaths.has(path) && isSizeInFlight(state, path),
  )
}

export function withoutStaleSizes(state: SizeRequestState, stalePaths: string[]): SizeRequestState {
  const pendingSizeRequests: PendingRequests = { ...state.pendingSizeRequests }
  const sizeStates: SizeStates = { ...state.sizeStates }
  for (const stalePath of stalePaths) {
    delete pendingSizeRequests[stalePath]
    const staleSize = sizeStates[stalePath]
    if (staleSize?.state === 'calculating') {
      delete sizeStates[stalePath]
      continue
    }
    if (staleSize?.refreshing) {
      sizeStates[stalePath] = { ...staleSize, refreshing: false }
    }
  }
  return { pendingSizeRequests, sizeStates }
}

function freshEntrySizeState(event: SizeStateEvent): EntrySizeState {
  return {
    state: event.state,
    sizeBytes: event.sizeBytes,
    source: event.source,
    naReason: event.naReason,
    isLowerBound: event.isLowerBound,
    skippedRoots: event.skippedRoots,
    excludedRoots: event.excludedRoots,
    skippedMore: event.skippedMore,
  }
}

function nextEntrySizeState(
  previous: EntrySizeState | undefined,
  event: SizeStateEvent,
): EntrySizeState {
  const recalculatesKnownValue = event.state === 'calculating' && previous?.state === 'ready'
  if (!recalculatesKnownValue) {
    return freshEntrySizeState(event)
  }
  return { ...previous, refreshing: true }
}

function focusedEntryPaths(panes: PaneRecord) {
  const paths = new Set<string>()
  for (const pane of Object.values(panes)) {
    const path = pane.entries.find((entry) => entry.id === pane.focusedEntryId)?.path
    if (path !== undefined) {
      paths.add(path)
    }
  }
  return paths
}

export function summarizeSizeEvents(events: SizeStateEvent[]): SizeEventBatchSummary {
  return {
    batchSize: events.length,
    calculatingCount: events.filter((event) => event.state === 'calculating').length,
    terminalCount: events.filter((event) => isTerminalSizeState(event.state)).length,
  }
}

export function appliedSizeStates(
  state: SizeRequestState & Pick<PanesData, 'panes'>,
  events: SizeStateEvent[],
): Partial<SizeRequestState> {
  if (events.length === 0) {
    return {}
  }

  const nextSizeStates = { ...state.sizeStates }
  for (const event of events) {
    nextSizeStates[event.path] = nextEntrySizeState(nextSizeStates[event.path], event)
  }
  const terminalPaths = events
    .filter((event) => isTerminalSizeState(event.state))
    .map((event) => event.path)

  return {
    pendingSizeRequests:
      terminalPaths.length > 0
        ? withKeyFlags(state.pendingSizeRequests, terminalPaths, false)
        : state.pendingSizeRequests,
    sizeStates: pruneSizeCache(nextSizeStates, focusedEntryPaths(state.panes)),
  }
}

function hasSettledSize(sizeStates: SizeStates, path: string) {
  const sizeState = sizeStates[path]?.state
  return sizeState !== undefined && isTerminalSizeState(sizeState)
}

export function refreshSizePaths(folders: string[], sizeStates: SizeStates, eager: boolean) {
  const eagerForPane = eager && folders.length <= AUTO_FOLDER_SIZE_MAX_DIRECTORIES
  if (eagerForPane) {
    return folders
  }
  return folders.filter((path) => hasSettledSize(sizeStates, path))
}
