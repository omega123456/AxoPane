import { createFile, createFolder, renameEntry } from '@/lib/ipc/commands'
import { activeTab } from '@/stores/tabs-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import type { PaneId } from '@/types/pane'
import type { DirectoryEntry } from '@/lib/types/ipc'

/**
 * The shared post-mutation step: apply the same incremental patch shape used by
 * the filesystem watcher so simple create/rename/delete actions do not force a
 * full pane reload.
 */
function patchAndFocus(
  paneId: PaneId,
  options: {
    changed?: DirectoryEntry[]
    removed?: string[]
    focusPath?: string
  },
) {
  const store = usePanesStore.getState()
  const pane = store.panes[paneId]

  store.applyDirPatch({
    tabId: activeTab(paneId).id,
    path: pane.path,
    reason: 'watch',
    changed: (options.changed ?? []).map((entry) => ({ path: entry.path, entry })),
    removed: options.removed ?? [],
  })

  const focusPath = options.focusPath
  if (!focusPath) {
    return
  }

  const entry = usePanesStore
    .getState()
    .panes[paneId].entries.find((item) => item.path === focusPath)
  if (entry) {
    usePanesStore.getState().setFocusedEntry(paneId, entry.id)
    useSelectionStore.getState().setSelection(paneId, [entry.id], entry.id, entry.id)
  }
}

export async function createFolderInPane(paneId: PaneId, name: string) {
  const parent = usePanesStore.getState().panes[paneId].path
  const entry = await createFolder({ parent, name })
  patchAndFocus(paneId, { changed: [entry], focusPath: entry.path })
}

export async function createFileInPane(paneId: PaneId, name: string) {
  const parent = usePanesStore.getState().panes[paneId].path
  const entry = await createFile({ parent, name })
  patchAndFocus(paneId, { changed: [entry], focusPath: entry.path })
}

export async function renameEntryInPane(paneId: PaneId, path: string, newName: string) {
  const entry = await renameEntry({ path, newName })
  patchAndFocus(paneId, { changed: [entry], removed: [path], focusPath: entry.path })
}
