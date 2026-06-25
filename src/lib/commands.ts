import { startOp } from '@/lib/queue-commands'
import { log } from '@/lib/app-log-commands'
import { persistAppConfig } from '@/lib/app-config'
import type { CommandId, DirectoryEntry } from '@/lib/types/ipc'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useSettingsStore } from '@/stores/settings-store'

export function executeCommand(commandId: CommandId, paneId: 'left' | 'right', targetEntryId?: string) {
  const panes = usePanesStore.getState()
  const pane = panes.panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  const entry = targetEntryId ? pane.entries.find((item) => item.id === targetEntryId) : undefined
  const selectedEntries = pane.entries.filter((item) => selection.selectedIds.includes(item.id))
  const effectiveEntries = entry ? [entry] : selectedEntries

  switch (commandId) {
    case 'open': {
      const focusId = targetEntryId ?? pane.focusedEntryId
      if (focusId) {
        const focused = pane.entries.find((item) => item.id === focusId)
        if (focused?.isDir) {
          void panes.navigatePane(paneId, focused.path)
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
        void panes.navigatePane(otherPaneId, entry.path)
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
    case 'toggleDetails':
      useLayoutStore.getState().setDetailsVisible(!useLayoutStore.getState().detailsVisible)
      void persistAppConfig()
      break
    case 'showSettings':
      useSettingsStore.getState().open('keybindings')
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
        useActionDialogStore.getState().open({
          kind: 'rename',
          paneId,
          entryId: target.id,
          path: target.path,
          initialValue: target.name,
        })
      }
      break
    }
    case 'delete': {
      if (effectiveEntries.length > 0) {
        useActionDialogStore.getState().open({
          kind: 'delete',
          paneId,
          targets: effectiveEntries.map((item) => ({ id: item.id, name: item.name, path: item.path })),
        })
      }
      break
    }
    case 'copy':
    case 'cut': {
      if (effectiveEntries.length > 0) {
        useClipboardStore.getState().setClipboard(commandId === 'copy' ? 'copy' : 'move', paneId, effectiveEntries)
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
        }
      })
      break
    }
    default:
      log.info('command not implemented', { commandId, paneId, targetEntryId, entries: effectiveEntries.length })
  }
}

export function canExecuteCommand(commandId: CommandId, paneId: 'left' | 'right', targetEntryId?: string) {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  const clipboard = useClipboardStore.getState()
  const entry = targetEntryId ? pane.entries.find((item) => item.id === targetEntryId) : undefined

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
      return Boolean(entry || selection.selectedIds.length > 0)
    default:
      return true
  }
}

export function selectedEntriesForPane(paneId: 'left' | 'right'): DirectoryEntry[] {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  return pane.entries.filter((entry) => selection.selectedIds.includes(entry.id))
}
