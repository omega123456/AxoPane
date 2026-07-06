import {
  closeTabContextAction,
  commandContextAction,
  compressContextAction,
  copyPathsContextAction,
  ejectVolumeContextAction,
  extractContextAction,
  navigateContextAction,
  openPathInExplicitPaneContextAction,
  openPathInNewTabContextAction,
  openWithContextAction,
  propertiesContextAction,
  requestFolderSizeContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { getFileCategory } from '@/lib/file-type'
import { findVolumeForPath, isVolumeRoot } from '@/lib/volumes'
import {
  createPathPropertiesDialogItem,
  createTrashPropertiesDialogItem,
  toPropertiesDialogItem,
} from '@/lib/properties-commands'
import { isTrashPath } from '@/lib/trash'
import type {
  ContextMenuActionRow,
  ContextMenuContent,
  ContextMenuIcon,
  ContextMenuRowItem,
  ContextMenuSection,
  ContextMenuStripItem,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import { commandLabels, formatShortcutLabel, type PlatformOs } from '@/lib/keymap'
import { selectedEntriesForPane } from '@/lib/commands'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { CommandId, DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

function shortcutFor(commandId: CommandId, os: PlatformOs) {
  const binding = useKeymapStore.getState().bindings[commandId][0]
  return binding ? formatShortcutLabel(binding, os) : undefined
}

const commandIcons: Partial<Record<CommandId, ContextMenuIcon>> = {
  calculateSize: { kind: 'app', name: 'calculate-size' },
  copy: { kind: 'app', name: 'copy' },
  cut: { kind: 'app', name: 'cut' },
  delete: { kind: 'app', name: 'delete' },
  deletePermanent: { kind: 'app', name: 'delete' },
  emptyTrash: { kind: 'app', name: 'empty-trash' },
  restore: { kind: 'app', name: 'restore' },
  newFile: { kind: 'app', name: 'new-file' },
  newFolder: { kind: 'app', name: 'new-folder' },
  open: { kind: 'app', name: 'open' },
  openInNewTab: { kind: 'app', name: 'open-in-new-tab' },
  openInOtherPane: { kind: 'app', name: 'open-in-other-pane' },
  paste: { kind: 'app', name: 'paste' },
  refresh: { kind: 'app', name: 'refresh' },
  rename: { kind: 'app', name: 'rename' },
  selectAll: { kind: 'app', name: 'select-all' },
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
    icon: commandIcons[commandId],
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
    icon?: ContextMenuIcon
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
    icon: options?.icon,
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

function isTrashPane(paneId: PaneId): boolean {
  return isTrashPath(usePanesStore.getState().panes[paneId].path)
}

function buildTrashEntryContent(
  target: Extract<ContextMenuTarget, { kind: 'file' | 'folder' }>,
  os: PlatformOs,
): ContextMenuContent {
  const targetEntryId = target.entry.id

  return {
    topStrip: [],
    sections: [
      section('primary', [
        commandRow('restore', os, {
          targetEntryId,
          strong: true,
          disabled: !target.entry.originalPath,
        }),
        commandRow('deletePermanent', os, { targetEntryId, danger: true }),
      ]),
    ],
  }
}

function buildTrashMultiContent(paneId: PaneId, os: PlatformOs): ContextMenuContent {
  const selectedEntries = selectedEntriesForPane(paneId)

  return {
    topStrip: [],
    sections: [
      section('primary', [
        commandRow('restore', os, {
          strong: true,
          disabled: selectedEntries.some((entry) => !entry.originalPath),
        }),
        commandRow('deletePermanent', os, { danger: true }),
      ]),
    ],
  }
}

function buildTrashEmptyContent(os: PlatformOs): ContextMenuContent {
  return {
    topStrip: [],
    sections: [section('primary', [commandRow('emptyTrash', os, { danger: true, strong: true })])],
  }
}

function buildFileOrFolderContent(
  paneId: PaneId,
  target: Extract<ContextMenuTarget, { kind: 'file' | 'folder' }>,
  os: PlatformOs,
): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashEntryContent(target, os)
  }

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
          { disabled: target.kind === 'folder', icon: { kind: 'app', name: 'open-with' } },
        ),
      ]),
      section('secondary', [
        commandRow('paste', os, { disabled: !canPaste }),
        customRow(
          `copy-path-${target.entry.id}`,
          'Copy as path',
          copyPathsContextAction([target.entry.path]),
          { icon: { kind: 'app', name: 'copy' } },
        ),
        customRow(
          `compress-${target.entry.id}`,
          'Compress',
          compressContextAction([target.entry.path], usePanesStore.getState().panes[paneId].path),
          { icon: { kind: 'app', name: 'archive' } },
        ),
        customRow(
          `extract-${target.entry.id}`,
          'Extract',
          extractContextAction([target.entry.path], usePanesStore.getState().panes[paneId].path),
          { disabled: !canExtract, icon: { kind: 'app', name: 'extract' } },
        ),
        ...(target.kind === 'folder' ? [calculateSizeRow(target.entry, os)] : []),
        commandRow('deletePermanent', os, { targetEntryId, danger: true }),
      ]),
      section('footer', [
        customRow(
          `properties-${target.entry.id}`,
          'Properties',
          propertiesContextAction([targetItem]),
          { icon: { kind: 'app', name: 'properties' } },
        ),
      ]),
    ],
  }
}

function buildMultiContent(paneId: PaneId, os: PlatformOs): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashMultiContent(paneId, os)
  }

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
          { disabled: selectedEntries.length === 0, icon: { kind: 'app', name: 'archive' } },
        ),
        customRow(
          'extract-selection',
          'Extract',
          extractContextAction(
            selectedEntries.map((entry) => entry.path),
            pane.path,
          ),
          { disabled: !canExtract, icon: { kind: 'app', name: 'extract' } },
        ),
        commandRow('paste', os, { disabled: clipboard.entries.length === 0 }),
        customRow(
          'copy-path-selection',
          'Copy as path',
          copyPathsContextAction(selectedEntries.map((entry) => entry.path)),
          { disabled: selectedEntries.length === 0, icon: { kind: 'app', name: 'copy' } },
        ),
        commandRow('deletePermanent', os, {
          danger: true,
          disabled: selectedEntries.length === 0,
        }),
        commandRow('selectAll', os, {
          disabled: pane.entries.length === 0,
        }),
      ]),
      section('footer', [
        customRow(
          'properties-selection',
          'Properties',
          propertiesContextAction(selectedEntries.map(toPropertiesDialogItem)),
          { disabled: selectedEntries.length === 0, icon: { kind: 'app', name: 'properties' } },
        ),
      ]),
    ],
  }
}

function buildEmptyContent(paneId: PaneId, os: PlatformOs): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashEmptyContent(os)
  }

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
          { icon: { kind: 'app', name: 'properties' } },
        ),
      ]),
    ],
  }
}

function buildTrashTreeContent(os: PlatformOs): ContextMenuContent {
  return {
    topStrip: [],
    sections: [
      section('primary', [commandRow('emptyTrash', os, { danger: true, strong: true })]),
      section('properties', [
        customRow(
          'properties-tree-trash',
          'Properties',
          propertiesContextAction([createTrashPropertiesDialogItem(os)]),
          { icon: { kind: 'app', name: 'properties' } },
        ),
      ]),
    ],
  }
}

function buildTreeOpenTargetRows(path: string): ContextMenuActionRow[] {
  if (useLayoutStore.getState().defaultPaneMode === 'single') {
    return [
      customRow(`open-tree-tab-${path}`, commandLabels.openInNewTab, openPathInNewTabContextAction(path), {
        icon: { kind: 'app', name: 'open-in-new-tab' },
      }),
    ]
  }

  return [
    customRow(
      `open-tree-right-${path}`,
      'Open in right pane',
      openPathInExplicitPaneContextAction(path, 'right'),
      { icon: { kind: 'app', name: 'open-in-right-pane' } },
    ),
    customRow(
      `open-tree-left-${path}`,
      'Open in left pane',
      openPathInExplicitPaneContextAction(path, 'left'),
      { icon: { kind: 'app', name: 'open-in-left-pane' } },
    ),
  ]
}

function buildTreeContent(
  target: Extract<ContextMenuTarget, { kind: 'tree' }>,
  os: PlatformOs,
): ContextMenuContent {
  if (isTrashPath(target.path)) {
    return buildTrashTreeContent(os)
  }

  const volumes = usePanesStore.getState().volumes
  const networkNode = volumes.find((volume) =>
    target.path.toLowerCase().startsWith(volume.mountRoot.toLowerCase()),
  )?.isNetwork

  const volume = findVolumeForPath(target.path, volumes)
  // Eject is macOS-only: on Windows a safe removal isn't possible from here (an
  // in-use volume can only be force-dismounted), so Windows users use the native
  // "Eject" entry in the shell context menu instead.
  const isEjectableRoot =
    os === 'macos' &&
    Boolean(volume && isVolumeRoot(target.path, volume) && volume.isRemovable)

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
            icon: { kind: 'app', name: 'open' },
          },
        ),
        ...buildTreeOpenTargetRows(target.path),
      ]),
      section('footer', [
        customRow(
          `copy-tree-path-${target.path}`,
          'Copy as path',
          copyPathsContextAction([target.path]),
          { icon: { kind: 'app', name: 'copy' } },
        ),
        customRow(
          `calculate-tree-${target.path}`,
          commandLabels.calculateSize,
          requestFolderSizeContextAction(target.path),
          {
            shortcut: shortcutFor('calculateSize', os),
            hidden: usePanesStore.getState().everythingStatus?.isAvailable ?? false,
            disabled: Boolean(networkNode),
            icon: { kind: 'app', name: 'calculate-size' },
          },
        ),
      ]),
      ...(isEjectableRoot && volume
        ? [
            section('eject', [
              customRow(
                `eject-tree-${target.path}`,
                'Eject',
                ejectVolumeContextAction(volume.mountRoot),
                { icon: { kind: 'app', name: 'eject' } },
              ),
            ]),
          ]
        : []),
      section('properties', [
        customRow(
          `properties-tree-${target.path}`,
          'Properties',
          propertiesContextAction([createPathPropertiesDialogItem(target.path)]),
          { icon: { kind: 'app', name: 'properties' } },
        ),
      ]),
    ],
  }
}

function buildTabContent(
  paneId: PaneId,
  target: Extract<ContextMenuTarget, { kind: 'tab' }>,
): ContextMenuContent {
  const tabs = useTabsStore.getState().panes[paneId]
  const path = usePanesStore.getState().panes[paneId].path

  return {
    topStrip: [],
    sections: [
      section('primary', [
        customRow(
          `open-tab-other-${target.tabId}`,
          commandLabels.openInOtherPane,
          navigateContextAction(path, 'other-pane'),
          { strong: true, icon: { kind: 'app', name: 'open-in-other-pane' } },
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
            icon: { kind: 'app', name: 'close-tab' },
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
      return buildTabContent(paneId, target)
  }
}
