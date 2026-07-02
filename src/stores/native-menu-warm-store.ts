import { create } from 'zustand'
import { log } from '@/lib/app-log-commands'
import { warmNativeMenus } from '@/lib/ipc/commands'
import { buildWarmRequestForEntry, nativeMenuTypeKeyForEntry } from '@/lib/context-menu/native-menu-type-key'
import { detectPlatformOs } from '@/lib/keymap'
import { isTrashPath } from '@/lib/trash'
import type { DirectoryEntry, LoadNativeMenuRequest } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'

/**
 * Session-scoped, in-memory record of native-menu cache type keys
 * (`file::<ext>` / `folder::<ext>`) already warmed (or currently in-flight)
 * this session. Mirrors the shape of `panes-store`'s `pendingIconRequests` /
 * `resolvedIconPaths` dedupe sets, but is kept in a dedicated store: warming
 * mutates no pane/entry state, so folding it into `panes-store` would be
 * incidental coupling.
 */
type WarmedTypeKeys = Record<string, true>

type NativeMenuWarmStore = {
  warmedTypeKeys: WarmedTypeKeys
  /**
   * Computes the distinct, not-yet-warmed native-menu type keys among the
   * given visible paths (resolved against the pane's current entries), marks
   * them warmed optimistically, and fires a single batch `warm_native_menus`
   * IPC request with one representative request per new key.
   *
   * No-ops on non-Windows platforms and for trash panes (native menus have no
   * valid filesystem target there), mirroring the interactive native-request
   * guard in `menu-definitions.ts`.
   *
   * On a transport error, the keys fired in that batch are rolled back so a
   * transient failure can be retried on a later visible-range update.
   */
  warmVisibleNativeMenus: (paneId: PaneId, paths: string[]) => Promise<void>
  /** Test-only escape hatch: clears the warmed-key set so tests can assert dedupe from a clean slate. */
  resetWarmedTypeKeys: () => void
}

export const useNativeMenuWarmStore = create<NativeMenuWarmStore>((set, get) => ({
  warmedTypeKeys: {},
  warmVisibleNativeMenus: async (paneId, paths) => {
    const os = detectPlatformOs()
    if (os !== 'windows') {
      log.debug('skipping native menu warm batch on unsupported platform', {
        paneId,
        os,
        visiblePathCount: paths.length,
      })
      return
    }

    const pane = usePanesStore.getState().panes[paneId]
    if (isTrashPath(pane.path)) {
      log.debug('skipping native menu warm batch for trash pane', {
        paneId,
        panePath: pane.path,
        visiblePathCount: paths.length,
      })
      return
    }

    const collected = collectWarmRequests(pane.entries, pane.path, paths, get().warmedTypeKeys)
    const { requests, skippedAlreadyWarmedOrInFlight, skippedDuplicateInBatch, skippedMissingEntry } =
      collected
    if (requests.length === 0) {
      log.debug('skipping native menu warm batch because every visible type was already handled', {
        paneId,
        panePath: pane.path,
        visiblePathCount: paths.length,
        skippedAlreadyWarmedOrInFlight,
        skippedDuplicateInBatch,
        skippedMissingEntry,
      })
      return
    }

    const newKeys = requests.map((entry) => entry.key)
    log.debug('dispatching native menu warm batch', {
      paneId,
      panePath: pane.path,
      visiblePathCount: paths.length,
      requestCount: requests.length,
      keys: newKeys,
      skippedAlreadyWarmedOrInFlight,
      skippedDuplicateInBatch,
      skippedMissingEntry,
    })

    set((state) => ({
      warmedTypeKeys: withWarmedTypeKeys(state.warmedTypeKeys, newKeys, true),
    }))

    try {
      await warmNativeMenus({ requests: requests.map((entry) => entry.request) })
      log.debug('native menu warm batch completed', {
        paneId,
        panePath: pane.path,
        requestCount: requests.length,
        keys: newKeys,
      })
    } catch (error) {
      set((state) => ({
        warmedTypeKeys: withWarmedTypeKeys(state.warmedTypeKeys, newKeys, false),
      }))
      log.warn('native menu warm batch failed; rolling back warmed keys', {
        paneId,
        panePath: pane.path,
        requestCount: requests.length,
        keys: newKeys,
        error: error instanceof Error ? error.message : String(error),
      })
      throw error
    }
  },
  resetWarmedTypeKeys: () => set({ warmedTypeKeys: {} }),
}))

type WarmRequestEntry = { key: string; request: LoadNativeMenuRequest }
type CollectWarmRequestsResult = {
  requests: WarmRequestEntry[]
  skippedAlreadyWarmedOrInFlight: number
  skippedDuplicateInBatch: number
  skippedMissingEntry: number
}

/**
 * Resolves `paths` against the pane's current entries, derives each entry's
 * type key, and keeps exactly one representative request per distinct key
 * that is neither already warmed this session nor already picked earlier in
 * this same batch.
 */
function collectWarmRequests(
  entries: DirectoryEntry[],
  panePath: string,
  paths: string[],
  warmedTypeKeys: WarmedTypeKeys,
): CollectWarmRequestsResult {
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

function withWarmedTypeKeys(
  warmedTypeKeys: WarmedTypeKeys,
  keys: string[],
  warmed: boolean,
): WarmedTypeKeys {
  const next = { ...warmedTypeKeys }
  for (const key of keys) {
    if (warmed) {
      next[key] = true
    } else {
      delete next[key]
    }
  }

  return next
}
