import { formatShortcutLabel, type PlatformOs } from '@/lib/keymap'
import { executeCommand, selectedEntriesForPane } from '@/lib/commands'
import { requestFolderSize } from '@/lib/ipc/commands'
import type { DirectoryEntry } from '@/lib/types/ipc'
import { useClipboardStore } from '@/stores/clipboard-store'
import type { ContextMenuItem } from '@/stores/context-menu-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneId } from '@/types/pane'

type MenuTarget =
  | { kind: 'file'; entry: DirectoryEntry }
  | { kind: 'folder'; entry: DirectoryEntry }
  | { kind: 'multi' }
  | { kind: 'empty' }
  | { kind: 'tab'; tabId: string }
  | { kind: 'tree'; path: string }

function shortcutFor(commandId: keyof ReturnType<typeof useKeymapStore.getState>['bindings'], os: PlatformOs) {
  const binding = useKeymapStore.getState().bindings[commandId][0]
  return binding ? formatShortcutLabel(binding, os) : undefined
}

function baseAction(
  paneId: PaneId,
  commandId: Parameters<typeof executeCommand>[0],
  os: PlatformOs,
  options?: { disabled?: boolean; danger?: boolean; targetEntryId?: string },
): ContextMenuItem {
  return {
    id: commandId,
    label: commandLabel(commandId),
    shortcut: shortcutFor(commandId, os),
    disabled: options?.disabled,
    danger: options?.danger,
    onSelect: () => executeCommand(commandId, paneId, options?.targetEntryId),
  }
}

function commandLabel(commandId: Parameters<typeof executeCommand>[0]) {
  const labels = {
    open: 'Open',
    goUp: 'Go up',
    refresh: 'Refresh',
    rename: 'Rename',
    delete: 'Delete',
    copy: 'Copy',
    cut: 'Cut',
    paste: 'Paste',
    newFolder: 'New folder',
    newFile: 'New file',
    calculateSize: 'Calculate size',
    openInNewTab: 'Open in new tab',
    openInOtherPane: 'Open in other pane',
    selectAll: 'Select all',
    clearFilter: 'Clear filter',
    toggleDetails: 'Toggle details',
    showSettings: 'Settings',
  }

  return labels[commandId]
}

function calculateSizeItem(paneId: PaneId, entry: DirectoryEntry, os: PlatformOs): ContextMenuItem {
  const everythingStatus = usePanesStore.getState().everythingStatus
  const volumes = usePanesStore.getState().volumes
  const volume = volumes.find((item) => entry.path.toLowerCase().startsWith(item.mountRoot.toLowerCase()))

  return {
    ...baseAction(paneId, 'calculateSize', os, {
      targetEntryId: entry.id,
      disabled: Boolean(volume?.isNetwork),
    }),
    hidden: everythingStatus?.isAvailable ?? false,
  }
}

export function buildContextMenuItems(paneId: PaneId, target: MenuTarget, os: PlatformOs): ContextMenuItem[] {
  const pane = usePanesStore.getState().panes[paneId]
  const clipboard = useClipboardStore.getState()
  const selectedEntries = selectedEntriesForPane(paneId)
  const canPaste = clipboard.entries.length > 0

  if (target.kind === 'tab') {
    const tabs = useTabsStore.getState().panes[paneId]
    return [
      {
        id: 'refresh-tab',
        label: 'Refresh',
        shortcut: shortcutFor('refresh', os),
        onSelect: () => executeCommand('refresh', paneId),
      },
      {
        id: 'close-tab',
        label: 'Close tab',
        disabled: tabs.tabs.length <= 1,
        onSelect: () => void usePanesStore.getState().closeTab(paneId, target.tabId),
      },
      {
        id: 'open-tab-other',
        label: 'Open in other pane',
        onSelect: () => void usePanesStore.getState().navigatePane(paneId === 'left' ? 'right' : 'left', pane.path),
      },
    ]
  }

  if (target.kind === 'tree') {
    const networkNode = usePanesStore
      .getState()
      .volumes.find((volume) => target.path.toLowerCase().startsWith(volume.mountRoot.toLowerCase()))?.isNetwork

    return [
      {
        id: 'open-tree',
        label: 'Open',
        shortcut: shortcutFor('open', os),
        onSelect: () => void usePanesStore.getState().navigatePane(paneId, target.path),
      },
      {
        id: 'open-tree-tab',
        label: 'Open in new tab',
        shortcut: shortcutFor('openInNewTab', os),
        onSelect: () => void usePanesStore.getState().openTabFromPath(paneId, target.path),
      },
      {
        id: 'open-tree-other',
        label: 'Open in other pane',
        shortcut: shortcutFor('openInOtherPane', os),
        onSelect: () => void usePanesStore.getState().navigatePane(paneId === 'left' ? 'right' : 'left', target.path),
      },
      {
        id: 'calculate-tree',
        label: 'Calculate size',
        shortcut: shortcutFor('calculateSize', os),
        hidden: usePanesStore.getState().everythingStatus?.isAvailable ?? false,
        disabled: Boolean(networkNode),
        onSelect: () => {
          if (!networkNode) {
            void requestFolderSize({ path: target.path })
          }
        },
      },
      baseAction(paneId, 'refresh', os),
    ]
  }

  if (target.kind === 'empty') {
    return [
      baseAction(paneId, 'paste', os, { disabled: !canPaste }),
      baseAction(paneId, 'newFolder', os),
      baseAction(paneId, 'newFile', os),
      baseAction(paneId, 'refresh', os),
      baseAction(paneId, 'selectAll', os, { disabled: pane.entries.length === 0 }),
    ]
  }

  if (target.kind === 'multi') {
    return [
      baseAction(paneId, 'copy', os, { disabled: selectedEntries.length === 0 }),
      baseAction(paneId, 'cut', os, { disabled: selectedEntries.length === 0 }),
      baseAction(paneId, 'paste', os, { disabled: !canPaste }),
      baseAction(paneId, 'delete', os, { disabled: selectedEntries.length === 0, danger: true }),
      baseAction(paneId, 'refresh', os),
      baseAction(paneId, 'selectAll', os, { disabled: pane.entries.length === 0 }),
    ]
  }

  const entry = target.entry
  const targetEntryId = entry.id
  const common = [
    baseAction(paneId, 'open', os, { targetEntryId }),
    baseAction(paneId, 'copy', os, { targetEntryId }),
    baseAction(paneId, 'cut', os, { targetEntryId }),
    baseAction(paneId, 'paste', os, { disabled: !canPaste }),
    baseAction(paneId, 'rename', os, { targetEntryId }),
    baseAction(paneId, 'delete', os, { targetEntryId, danger: true }),
  ]

  if (target.kind === 'folder') {
    return [
      ...common,
      calculateSizeItem(paneId, entry, os),
      baseAction(paneId, 'openInNewTab', os, { targetEntryId }),
      baseAction(paneId, 'openInOtherPane', os, { targetEntryId }),
      baseAction(paneId, 'refresh', os),
    ]
  }

  return [
    ...common,
    baseAction(paneId, 'openInNewTab', os, { targetEntryId, disabled: true }),
    baseAction(paneId, 'openInOtherPane', os, { targetEntryId, disabled: true }),
    baseAction(paneId, 'refresh', os),
  ]
}

export function resolveMenuTarget(paneId: PaneId, entry?: DirectoryEntry): MenuTarget {
  const selection = useSelectionStore.getState().selections[paneId]

  if (!entry && selection.selectedIds.length > 1) {
    return { kind: 'multi' }
  }

  if (!entry) {
    return selection.selectedIds.length > 1 ? { kind: 'multi' } : { kind: 'empty' }
  }

  const multipleSelected =
    selection.selectedIds.length > 1 && selection.selectedIds.includes(entry.id)
  if (multipleSelected) {
    return { kind: 'multi' }
  }

  if (entry.isDir) {
    return { kind: 'folder', entry }
  }

  return { kind: 'file', entry }
}
