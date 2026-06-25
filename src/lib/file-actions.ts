import { createFile, createFolder, deleteEntries, renameEntry } from '@/lib/ipc/commands'
import { usePanesStore } from '@/stores/panes-store'
import { clearSelectionForPane, useSelectionStore } from '@/stores/selection-store'
import type { DeleteTarget } from '@/stores/action-dialog-store'
import type { PaneId } from '@/types/pane'

/**
 * The shared post-mutation step: reload the affected pane so the new listing is
 * authoritative (the fs-watcher also patches in the real app, but an explicit
 * reload makes the change appear immediately and deterministically in tests and
 * the web build where no watcher runs).
 */
async function reloadAndFocus(paneId: PaneId, focusPath?: string) {
  await usePanesStore.getState().reloadPane(paneId)
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
  await reloadAndFocus(paneId, entry.path)
}

export async function createFileInPane(paneId: PaneId, name: string) {
  const parent = usePanesStore.getState().panes[paneId].path
  const entry = await createFile({ parent, name })
  await reloadAndFocus(paneId, entry.path)
}

export async function renameEntryInPane(paneId: PaneId, path: string, newName: string) {
  const entry = await renameEntry({ path, newName })
  await reloadAndFocus(paneId, entry.path)
}

export async function deleteEntriesInPane(paneId: PaneId, targets: DeleteTarget[]) {
  await deleteEntries({ paths: targets.map((target) => target.path) })
  clearSelectionForPane(paneId)
  await usePanesStore.getState().reloadPane(paneId)
}
