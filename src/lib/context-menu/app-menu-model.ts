import {
  closeTabContextAction,
  commandContextAction,
  compressContextAction,
  extractContextAction,
  navigateContextAction,
  openPathInNewTabContextAction,
  openWithContextAction,
  propertiesContextAction,
  requestFolderSizeContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { getFileCategory } from '@/lib/file-type'
import {
  createPathPropertiesDialogItem,
  toPropertiesDialogItem,
} from '@/lib/properties-commands'
import type {
  ContextMenuActionRow,
  ContextMenuContent,
  ContextMenuRowItem,
  ContextMenuSection,
  ContextMenuStripItem,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import { commandLabels, formatShortcutLabel, type PlatformOs } from '@/lib/keymap'
import { selectedEntriesForPane } from '@/lib/commands'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { CommandId, DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

function shortcutFor(commandId: CommandId, os: PlatformOs) {
  const binding = useKeymapStore.getState().bindings[commandId][0]
  return binding ? formatShortcutLabel(binding, os) : undefined
}

function stripCommand(
  commandId: CommandId,
  options: {
    icon: ContextMenuStripItem['icon']
    label?: string
    targetEntryId?: string
    disabled?: boolean
    danger?: boolean
    hidden?: boolean
  },
): ContextMenuStripItem {
  return {
    id: `strip-${commandId}-${options.targetEntryId ?? 'menu'}`,
    label: options.label ?? commandLabels[commandId],
    owner: 'app',
    icon: options.icon,
    disabled: options.disabled,
    danger: options.danger,
    hidden: options.hidden,
    action: commandContextAction(commandId, options.targetEntryId),
  }
}

function commandRow(
  commandId: CommandId,
  os: PlatformOs,
  options?: {
    targetEntryId?: string
    disabled?: boolean
    danger?: boolean
    hidden?: boolean
    strong?: boolean
  },
): ContextMenuActionRow {
  return {
    id: `${commandId}-${options?.targetEntryId ?? 'menu'}`,
    kind: 'action',
    label: commandLabels[commandId],
    owner: 'app',
    shortcut: shortcutFor(commandId, os),
    disabled: options?.disabled,
    danger: options?.danger,
    hidden: options?.hidden,
    strong: options?.strong,
    action: commandContextAction(commandId, options?.targetEntryId),
  }
}

function customRow(
  id: string,
  label: string,
  action: ContextMenuActionRow['action'],
  options?: {
    disabled?: boolean
    danger?: boolean
    hidden?: boolean
    strong?: boolean
    shortcut?: string
  },
): ContextMenuActionRow {
  return {
    id,
    kind: 'action',
    label,
    owner: 'app',
    disabled: options?.disabled,
    danger: options?.danger,
    hidden: options?.hidden,
    strong: options?.strong,
    shortcut: options?.shortcut,
    action,
  }
}

function section(id: string, rows: ContextMenuRowItem[]): ContextMenuSection {
  return { id, rows }
}

function fileSystemStrip(
  targetEntryId: string | undefined,
  options?: {
    includeRename?: boolean
    includeDelete?: boolean
    copyDisabled?: boolean
    cutDisabled?: boolean
    renameDisabled?: boolean
    deleteDisabled?: boolean
  },
): ContextMenuStripItem[] {
  return [
    stripCommand('cut', {
      targetEntryId,
      icon: { kind: 'app', name: 'cut' },
      disabled: options?.cutDisabled,
    }),
    stripCommand('copy', {
      targetEntryId,
      icon: { kind: 'app', name: 'copy' },
      disabled: options?.copyDisabled,
    }),
    ...(options?.includeRename === false
      ? []
      : [
          stripCommand('rename', {
            targetEntryId,
            icon: { kind: 'app', name: 'rename' },
            disabled: options?.renameDisabled,
          }),
        ]),
    ...(options?.includeDelete === false
      ? []
      : [
          stripCommand('delete', {
            targetEntryId,
            icon: { kind: 'app', name: 'delete' },
            disabled: options?.deleteDisabled,
            danger: true,
          }),
        ]),
  ]
}

function calculateSizeRow(
  entry: DirectoryEntry,
  os: PlatformOs,
): ContextMenuActionRow {
  const everythingStatus = usePanesStore.getState().everythingStatus
  const volumes = usePanesStore.getState().volumes
  const volume = volumes.find((item) =>
    entry.path.toLowerCase().startsWith(item.mountRoot.toLowerCase()),
  )

  return commandRow('calculateSize', os, {
    targetEntryId: entry.id,
    hidden: everythingStatus?.isAvailable ?? false,
    disabled: Boolean(volume?.isNetwork),
  })
}

function isArchiveEntry(entry: DirectoryEntry) {
  return getFileCategory(entry) === 'archive'
}

function isZipArchiveEntry(entry: DirectoryEntry) {
  if (entry.isDir || !isArchiveEntry(entry)) {
    return false
  }

  return entry.name.toLowerCase().endsWith('.zip')
}

function buildFileOrFolderContent(
  paneId: PaneId,
  target: Extract<ContextMenuTarget, { kind: 'file' | 'folder' }>,
  os: PlatformOs,
): ContextMenuContent {
  const clipboard = useClipboardStore.getState()
  const canPaste = clipboard.entries.length > 0
  const targetEntryId = target.entry.id
  const targetItem = toPropertiesDialogItem(target.entry)
  const canExtract = isZipArchiveEntry(target.entry)

  return {
    topStrip: fileSystemStrip(targetEntryId),
    sections: [
      section('primary', [
        commandRow('open', os, { targetEntryId, strong: true }),
        commandRow('openInNewTab', os, {
          targetEntryId,
          disabled: target.kind === 'file',
        }),
        commandRow('openInOtherPane', os, {
          targetEntryId,
          disabled: target.kind === 'file',
        }),
        customRow(
          `open-with-${target.entry.id}`,
          'Open with',
          openWithContextAction(target.entry.path),
          { disabled: target.kind === 'folder' },
        ),
      ]),
      section('secondary', [
        commandRow('paste', os, { disabled: !canPaste }),
        customRow(
          `compress-${target.entry.id}`,
          'Compress',
          compressContextAction([target.entry.path], usePanesStore.getState().panes[paneId].path),
        ),
        customRow(
          `extract-${target.entry.id}`,
          'Extract',
          extractContextAction([target.entry.path], usePanesStore.getState().panes[paneId].path),
          { disabled: !canExtract },
        ),
        ...(target.kind === 'folder' ? [calculateSizeRow(target.entry, os)] : []),
        commandRow('deletePermanent', os, { targetEntryId, danger: true }),
        commandRow('refresh', os),
      ]),
      section('footer', [
        customRow(
          `properties-${target.entry.id}`,
          'Properties',
          propertiesContextAction([targetItem]),
        ),
      ]),
    ],
  }
}

function buildMultiContent(paneId: PaneId, os: PlatformOs): ContextMenuContent {
  const pane = usePanesStore.getState().panes[paneId]
  const clipboard = useClipboardStore.getState()
  const selectedEntries = selectedEntriesForPane(paneId)
  const canExtract = selectedEntries.length > 0 && selectedEntries.every(isZipArchiveEntry)

  return {
    topStrip: fileSystemStrip(
      undefined,
      {
        includeRename: false,
        copyDisabled: selectedEntries.length === 0,
        cutDisabled: selectedEntries.length === 0,
        deleteDisabled: selectedEntries.length === 0,
      },
    ),
    sections: [
      section('selection', [
        customRow(
          'compress-selection',
          'Compress',
          compressContextAction(
            selectedEntries.map((entry) => entry.path),
            pane.path,
          ),
          { disabled: selectedEntries.length === 0 },
        ),
        customRow(
          'extract-selection',
          'Extract',
          extractContextAction(
            selectedEntries.map((entry) => entry.path),
            pane.path,
          ),
          { disabled: !canExtract },
        ),
        commandRow('paste', os, { disabled: clipboard.entries.length === 0 }),
        commandRow('deletePermanent', os, {
          danger: true,
          disabled: selectedEntries.length === 0,
        }),
        commandRow('refresh', os),
        commandRow('selectAll', os, {
          disabled: pane.entries.length === 0,
        }),
      ]),
      section('footer', [
        customRow(
          'properties-selection',
          'Properties',
          propertiesContextAction(selectedEntries.map(toPropertiesDialogItem)),
          { disabled: selectedEntries.length === 0 },
        ),
      ]),
    ],
  }
}

function buildEmptyContent(paneId: PaneId, os: PlatformOs): ContextMenuContent {
  const pane = usePanesStore.getState().panes[paneId]
  const clipboard = useClipboardStore.getState()

  return {
    topStrip: [],
    sections: [
      section('create', [
        commandRow('paste', os, { disabled: clipboard.entries.length === 0 }),
        commandRow('newFolder', os),
        commandRow('newFile', os),
      ]),
      section('footer', [
        commandRow('refresh', os),
        commandRow('selectAll', os, {
          disabled: pane.entries.length === 0,
        }),
      ]),
      section('properties', [
        customRow(
          `properties-empty-${pane.path}`,
          'Properties',
          propertiesContextAction([createPathPropertiesDialogItem(pane.path)]),
        ),
      ]),
    ],
  }
}

function buildTreeContent(
  target: Extract<ContextMenuTarget, { kind: 'tree' }>,
  os: PlatformOs,
): ContextMenuContent {
  const networkNode = usePanesStore
    .getState()
    .volumes.find((volume) =>
      target.path.toLowerCase().startsWith(volume.mountRoot.toLowerCase()),
    )?.isNetwork

  return {
    topStrip: [],
    sections: [
      section('primary', [
        customRow(
          `open-tree-${target.path}`,
          commandLabels.open,
          navigateContextAction(target.path, 'current-pane'),
          {
            shortcut: shortcutFor('open', os),
            strong: true,
          },
        ),
        customRow(
          `open-tree-tab-${target.path}`,
          commandLabels.openInNewTab,
          openPathInNewTabContextAction(target.path),
          {
            shortcut: shortcutFor('openInNewTab', os),
          },
        ),
        customRow(
          `open-tree-other-${target.path}`,
          commandLabels.openInOtherPane,
          navigateContextAction(target.path, 'other-pane'),
          {
            shortcut: shortcutFor('openInOtherPane', os),
          },
        ),
      ]),
      section('footer', [
        customRow(
          `calculate-tree-${target.path}`,
          commandLabels.calculateSize,
          requestFolderSizeContextAction(target.path),
          {
            shortcut: shortcutFor('calculateSize', os),
            hidden: usePanesStore.getState().everythingStatus?.isAvailable ?? false,
            disabled: Boolean(networkNode),
          },
        ),
        commandRow('refresh', os),
      ]),
      section('properties', [
        customRow(
          `properties-tree-${target.path}`,
          'Properties',
          propertiesContextAction([createPathPropertiesDialogItem(target.path)]),
        ),
      ]),
    ],
  }
}

function buildTabContent(
  paneId: PaneId,
  target: Extract<ContextMenuTarget, { kind: 'tab' }>,
  os: PlatformOs,
): ContextMenuContent {
  const tabs = useTabsStore.getState().panes[paneId]
  const path = usePanesStore.getState().panes[paneId].path

  return {
    topStrip: [],
    sections: [
      section('primary', [
        commandRow('refresh', os, { strong: true }),
        customRow(
          `open-tab-other-${target.tabId}`,
          commandLabels.openInOtherPane,
          navigateContextAction(path, 'other-pane'),
        ),
      ]),
      section('footer', [
        customRow(
          `close-tab-${target.tabId}`,
          'Close tab',
          closeTabContextAction(target.tabId),
          {
            disabled: tabs.tabs.length <= 1,
            danger: true,
          },
        ),
      ]),
    ],
  }
}

export function buildAppContextMenuContent(
  paneId: PaneId,
  target: ContextMenuTarget,
  os: PlatformOs,
): ContextMenuContent {
  switch (target.kind) {
    case 'file':
    case 'folder':
      return buildFileOrFolderContent(paneId, target, os)
    case 'multi':
      return buildMultiContent(paneId, os)
    case 'empty':
      return buildEmptyContent(paneId, os)
    case 'tree':
      return buildTreeContent(target, os)
    case 'tab':
      return buildTabContent(paneId, target, os)
  }
}
