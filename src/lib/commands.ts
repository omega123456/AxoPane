import { startOp } from '@/lib/queue-commands'
import { log } from '@/lib/app-log-commands'
import {
  clearFileClipboard,
  moveToTrash,
  openPath,
  restoreFromTrash,
  writeFileClipboard,
} from '@/lib/ipc/commands'
import type { CommandId, DirectoryEntry } from '@/lib/types/ipc'
import { isTrashPath } from '@/lib/trash'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useErrorToastStore } from '@/stores/error-toast-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useSettingsStore } from '@/stores/settings-store'

const PARENT_ROW_ID = '..'

// The trash browser has no real filesystem entries backing it (see
// `trashEntryToDirectoryEntry`), so most file commands (rename, cut/copy,
// paste, new folder/file, open, calculate size, …) can't act on it. Only
// these make sense there.
const trashPaneCommands = new Set<CommandId>([
  'restore',
  'emptyTrash',
  'delete',
  'deletePermanent',
  'refresh',
  'selectAll',
  'clearFilter',
  'showSettings',
])

export function executeCommand(
  commandId: CommandId,
  paneId: 'left' | 'right',
  targetEntryId?: string,
) {
  const panes = usePanesStore.getState()
  const pane = panes.panes[paneId]
  const inTrashPane = isTrashPath(pane.path)
  const isTrashOnlyCommand = commandId === 'restore' || commandId === 'emptyTrash'
  if (inTrashPane && !trashPaneCommands.has(commandId)) {
    return
  }
  if (!inTrashPane && isTrashOnlyCommand) {
    return
  }

  const selection = useSelectionStore.getState().selections[paneId]
  const entry = targetEntryId ? pane.entries.find((item) => item.id === targetEntryId) : undefined
  const selectedEntries = pane.entries.filter((item) => selection.selectedIds.includes(item.id))
  const effectiveEntries = entry ? [entry] : selectedEntries
  const focusedEntry = pane.focusedEntryId
    ? pane.entries.find((item) => item.id === pane.focusedEntryId)
    : undefined
  const transferEntries = entry
    ? [entry]
    : selectedEntries.length > 0
      ? selectedEntries
      : focusedEntry
        ? [focusedEntry]
        : []

  switch (commandId) {
    case 'open': {
      const focusId = targetEntryId ?? pane.focusedEntryId
      if (focusId) {
        if (focusId === PARENT_ROW_ID) {
          void panes.goUp(paneId)
          break
        }

        const focused = pane.entries.find((item) => item.id === focusId)
        if (focused?.isDir) {
          void panes.navigatePane(paneId, focused.path)
        } else if (focused) {
          void openPath({ path: focused.path }).catch((error) => {
            log.error('open command failed', {
              paneId,
              path: focused.path,
              error,
            })
          })
        }
      }
      break
    }
    case 'goUp':
      void panes.goUp(paneId)
      break
    case 'refresh':
      void panes.refreshEverything(paneId)
      break
    case 'calculateSize': {
      const focusId = targetEntryId ?? pane.focusedEntryId
      if (focusId) {
        void panes.requestManualSize(paneId, focusId)
      }
      break
    }
    case 'openInNewTab':
      if (entry?.isDir) {
        void panes.openTabFromPath(paneId, entry.path)
      }
      break
    case 'openInOtherPane':
      if (entry?.isDir) {
        const otherPaneId = paneId === 'left' ? 'right' : 'left'
        void panes.openTabFromPath(otherPaneId, entry.path)
      }
      break
    case 'selectAll':
      useSelectionStore.getState().setSelection(
        paneId,
        pane.entries.map((item) => item.id),
        pane.entries[0]?.id ?? null,
        pane.entries[0]?.id ?? null,
      )
      break
    case 'clearFilter':
      panes.clearFilter(paneId)
      break
    case 'showSettings':
      useSettingsStore.getState().open()
      break
    case 'newFolder':
      useActionDialogStore.getState().open({ kind: 'newFolder', paneId })
      break
    case 'newFile':
      useActionDialogStore.getState().open({ kind: 'newFile', paneId })
      break
    case 'rename': {
      // Rename targets exactly one item: the right-clicked entry, or the sole
      // selected entry when invoked from the keyboard.
      const target = entry ?? (selectedEntries.length === 1 ? selectedEntries[0] : undefined)
      if (target) {
        useInlineRenameStore.getState().beginRename({
          paneId,
          entryId: target.id,
          path: target.path,
          initialValue: target.name,
        })
      }
      break
    }
    case 'delete': {
      // Items already sitting in the trash have nowhere softer to go, so
      // Delete permanently removes them (confirmed first, like Shift+Delete).
      if (inTrashPane) {
        if (effectiveEntries.length > 0) {
          useActionDialogStore.getState().open({
            kind: 'deleteFromTrash',
            paneId,
            targets: effectiveEntries.map((item) => ({
              id: item.id,
              name: item.name,
              path: item.path,
              sizeBytes: item.sizeBytes,
            })),
          })
        }
        break
      }
      // Default delete moves to the OS bin/Recycle Bin/Trash (reversible, no
      // confirmation — matching Explorer/Finder). The fs watcher refreshes the
      // listing once the items disappear.
      if (effectiveEntries.length > 0) {
        void moveToTrash({ paths: effectiveEntries.map((item) => item.path) }).catch((error) => {
          log.error('move to trash failed', {
            paneId,
            paths: effectiveEntries.map((item) => item.path),
            error,
          })
        })
      }
      break
    }
    case 'deletePermanent': {
      if (effectiveEntries.length === 0) {
        break
      }
      // Irreversible hard delete: confirm first. Real files go through the
      // queue engine (shared toast/progress + per-disk lock); trash entries
      // have no real filesystem path to hand it, so they use their own
      // dialog kind that calls the trash-specific delete command directly.
      useActionDialogStore.getState().open(
        inTrashPane
          ? {
              kind: 'deleteFromTrash',
              paneId,
              targets: effectiveEntries.map((item) => ({
                id: item.id,
                name: item.name,
                path: item.path,
                sizeBytes: item.sizeBytes,
              })),
            }
          : {
              kind: 'delete',
              paneId,
              targets: effectiveEntries.map((item) => ({
                id: item.id,
                name: item.name,
                path: item.path,
                sizeBytes: item.sizeBytes,
              })),
            },
      )
      break
    }
    case 'restore': {
      const ids = effectiveEntries
        .map((item) => item.trashId)
        .filter((id): id is string => Boolean(id))
      if (ids.length > 0) {
        void restoreFromTrash({ ids })
          .then(() => panes.reloadPane(paneId))
          .catch((error) => {
            const message = error instanceof Error ? error.message : String(error)
            log.error('restore from trash failed', { paneId, ids, error })
            useErrorToastStore.getState().show(message)
          })
      }
      break
    }
    case 'emptyTrash': {
      if (pane.entries.length > 0) {
        useActionDialogStore.getState().open({
          kind: 'emptyTrash',
          paneId,
          count: pane.entries.length,
        })
      }
      break
    }
    case 'copy':
    case 'cut': {
      if (effectiveEntries.length > 0) {
        const mode = commandId === 'copy' ? 'copy' : 'move'
        useClipboardStore
          .getState()
          .setClipboard(mode, paneId, effectiveEntries)
        void writeFileClipboard({
          mode,
          paths: effectiveEntries.map((item) => item.path),
        }).catch((error) => {
          log.warn('write file clipboard failed', {
            paneId,
            mode,
            paths: effectiveEntries.map((item) => item.path),
            error,
          })
        })
      }
      break
    }
    case 'paste': {
      const clipboard = useClipboardStore.getState()
      if (!clipboard.mode || clipboard.entries.length === 0) {
        break
      }

      void startOp({
        kind: clipboard.mode,
        destinationDir: pane.path,
        items: clipboard.entries.map((item) => ({
          sourcePath: item.path,
          name: item.name,
          sizeBytes: item.sizeBytes ?? 0,
        })),
      }).then(() => {
        if (clipboard.mode === 'move') {
          useClipboardStore.getState().clearClipboard()
          void clearFileClipboard().catch((error) => {
            log.warn('clear file clipboard failed', {
              paneId,
              error,
            })
          })
        }
      })
      break
    }
    case 'copyToOtherPane':
    case 'moveToOtherPane': {
      if (transferEntries.length === 0) {
        break
      }

      const otherPaneId = paneId === 'left' ? 'right' : 'left'
      const destinationDir = panes.panes[otherPaneId].path

      useActionDialogStore.getState().open({
        kind: 'transferConfirm',
        paneId,
        operation: commandId === 'copyToOtherPane' ? 'copy' : 'move',
        sourceDir: pane.path,
        destinationDir,
        targets: transferEntries.map((item) => ({
          id: item.id,
          name: item.name,
          path: item.path,
          sizeBytes: item.sizeBytes,
        })),
      })
      break
    }
    default:
      log.info('command not implemented', {
        commandId,
        paneId,
        targetEntryId,
        entries: effectiveEntries.length,
      })
  }
}

export function canExecuteCommand(
  commandId: CommandId,
  paneId: 'left' | 'right',
  targetEntryId?: string,
) {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  const clipboard = useClipboardStore.getState()
  const entry = targetEntryId ? pane.entries.find((item) => item.id === targetEntryId) : undefined
  const inTrashPane = isTrashPath(pane.path)

  if (inTrashPane && !trashPaneCommands.has(commandId)) {
    return false
  }
  if (!inTrashPane && (commandId === 'restore' || commandId === 'emptyTrash')) {
    return false
  }

  switch (commandId) {
    case 'paste':
      return clipboard.entries.length > 0
    case 'selectAll':
      return pane.entries.length > 0
    case 'openInNewTab':
    case 'openInOtherPane':
    case 'calculateSize':
      return Boolean(entry?.isDir)
    case 'copy':
    case 'cut':
    case 'rename':
    case 'delete':
    case 'deletePermanent':
      return Boolean(entry || selection.selectedIds.length > 0)
    case 'restore': {
      const effectiveEntries = entry ? [entry] : selectedEntriesForPane(paneId)
      return effectiveEntries.length > 0 && effectiveEntries.every((item) => item.originalPath)
    }
    case 'emptyTrash':
      return pane.entries.length > 0
    case 'copyToOtherPane':
    case 'moveToOtherPane':
      return Boolean(entry || selection.selectedIds.length > 0 || pane.focusedEntryId)
    default:
      return true
  }
}

export function selectedEntriesForPane(paneId: 'left' | 'right'): DirectoryEntry[] {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  return pane.entries.filter((entry) => selection.selectedIds.includes(entry.id))
}
