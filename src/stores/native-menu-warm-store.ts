import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { warmNativeMenus } from '@/lib/ipc/commands'
import {
  buildWarmRequestForEntry,
  nativeMenuTypeKeyForEntry,
} from '@/lib/context-menu/native-menu-type-key'
import { detectPlatformOs } from '@/lib/keymap'
import { isTrashPath } from '@/lib/trash'
import type { DirectoryEntry, LoadNativeMenuRequest } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'
import { withKeyFlags } from '@/lib/collections'
import { errorMessage } from '@/lib/error-message'

type WarmedTypeKeys = Record<string, true>

type NativeMenuWarmStore = {
  warmedTypeKeys: WarmedTypeKeys
  warmVisibleNativeMenus: (paneId: PaneId, paths: string[]) => Promise<void>
  resetWarmedTypeKeys: () => void
}

type WarmStoreSet = (update: (state: NativeMenuWarmStore) => Partial<NativeMenuWarmStore>) => void

type WarmBatchContext = {
  paneId: PaneId
  panePath: string
  visiblePathCount: number
}

function warmablePanePath(paneId: PaneId, visiblePathCount: number): string | null {
  const platform = detectPlatformOs()
  if (platform !== 'windows') {
    log.debug('skipping native menu warm batch on unsupported platform', {
      paneId,
      platform,
      visiblePathCount,
    })
    return null
  }

  const pane = usePanesStore.getState().panes[paneId]
  if (isTrashPath(pane.path)) {
    log.debug('skipping native menu warm batch for trash pane', {
      paneId,
      panePath: pane.path,
      visiblePathCount,
    })
    return null
  }
  return pane.path
}

type VisiblePanePaths = {
  paneId: PaneId
  paths: string[]
}

async function warmVisibleNativeMenus(
  set: WarmStoreSet,
  warmedTypeKeys: WarmedTypeKeys,
  { paneId, paths }: VisiblePanePaths,
) {
  const panePath = warmablePanePath(paneId, paths.length)
  if (panePath === null) {
    return
  }

  const context = { paneId, panePath, visiblePathCount: paths.length }
  const pane = usePanesStore.getState().panes[paneId]
  const collected = collectWarmRequests({ entries: pane.entries, panePath, paths }, warmedTypeKeys)
  const { requests, ...skipped } = collected
  if (requests.length === 0) {
    log.debug('skipping native menu warm batch because every visible type was already handled', {
      ...context,
      ...skipped,
    })
    return
  }

  await dispatchWarmBatch(set, context, collected)
}

async function dispatchWarmBatch(
  set: WarmStoreSet,
  context: WarmBatchContext,
  collected: CollectWarmRequestsResult,
) {
  const { requests, ...skipped } = collected
  const newKeys = requests.map((entry) => entry.key)
  const { visiblePathCount, ...batchContext } = context
  log.debug('dispatching native menu warm batch', {
    ...batchContext,
    visiblePathCount,
    requestCount: requests.length,
    keys: newKeys,
    ...skipped,
  })

  set((state) => ({
    warmedTypeKeys: withKeyFlags(state.warmedTypeKeys, newKeys, true),
  }))

  const outcome = { ...batchContext, requestCount: requests.length, keys: newKeys }
  try {
    await warmNativeMenus({ requests: requests.map((entry) => entry.request) })
    log.debug('native menu warm batch completed', outcome)
  } catch (error) {
    set((state) => ({
      warmedTypeKeys: withKeyFlags(state.warmedTypeKeys, newKeys, false),
    }))
    log.warn('native menu warm batch failed; rolling back warmed keys', {
      ...outcome,
      error: errorMessage(error),
    })
    throw error
  }
}

export const useNativeMenuWarmStore = create<NativeMenuWarmStore>((set, get) => ({
  warmedTypeKeys: {},
  warmVisibleNativeMenus: (paneId, paths) =>
    warmVisibleNativeMenus(set, get().warmedTypeKeys, { paneId, paths }),
  resetWarmedTypeKeys: () => set({ warmedTypeKeys: {} }),
}))

type WarmRequestEntry = { key: string; request: LoadNativeMenuRequest }
type CollectWarmRequestsResult = {
  requests: WarmRequestEntry[]
  skippedAlreadyWarmedOrInFlight: number
  skippedDuplicateInBatch: number
  skippedMissingEntry: number
}

type WarmCandidates = {
  entries: DirectoryEntry[]
  panePath: string
  paths: string[]
}

function collectWarmRequests(
  candidates: WarmCandidates,
  warmedTypeKeys: WarmedTypeKeys,
): CollectWarmRequestsResult {
  const { entries, panePath, paths } = candidates
  const entryByPath = new Map(entries.map((entry) => [entry.path, entry]))
  const seenKeys = new Set<string>()
  const result: WarmRequestEntry[] = []
  let skippedAlreadyWarmedOrInFlight = 0
  let skippedDuplicateInBatch = 0
  let skippedMissingEntry = 0

  for (const path of paths) {
    const entry = entryByPath.get(path)
    if (entry === undefined) {
      skippedMissingEntry += 1
      continue
    }

    const key = nativeMenuTypeKeyForEntry(entry)
    if (warmedTypeKeys[key]) {
      skippedAlreadyWarmedOrInFlight += 1
      continue
    }
    if (seenKeys.has(key)) {
      skippedDuplicateInBatch += 1
      continue
    }
    seenKeys.add(key)

    result.push({ key, request: buildWarmRequestForEntry(entry, panePath, `warm:${key}`) })
  }

  return {
    requests: result,
    skippedAlreadyWarmedOrInFlight,
    skippedDuplicateInBatch,
    skippedMissingEntry,
  }
}
