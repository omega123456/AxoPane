import {
  compressContextAction,
  copyPathsContextAction,
  extractContextAction,
  openWithContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { createPathPropertiesDialogItem, toPropertiesDialogItem } from '@/lib/properties-commands'
import type {
  ContextMenuContent,
  ContextMenuSection,
  ContextMenuTarget,
} from '@/lib/types/context-menu'
import { type PlatformOs } from '@/lib/keymap'
import { selectedEntriesForPane } from '@/lib/commands'
import { hasClipboardEntries } from '@/stores/clipboard-store'
import { usePanesStore } from '@/stores/panes-store'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'

import {
  calculateSizeRow,
  commandRow,
  customRow,
  favouriteRow,
  fileSystemStrip,
  isTrashPane,
  isZipArchiveEntry,
  propertiesRow,
  section,
} from '@/lib/context-menu/menu-rows'

function buildTrashEntryContent(
  target: Extract<ContextMenuTarget, { kind: 'file' | 'folder' }>,
  platform: PlatformOs,
): ContextMenuContent {
  const targetEntryId = target.entry.id

  return {
    topStrip: [],
    sections: [
      section('primary', [
        commandRow('restore', platform, {
          targetEntryId,
          strong: true,
          disabled: !target.entry.originalPath,
        }),
        commandRow('deletePermanent', platform, { targetEntryId, danger: true }),
      ]),
    ],
  }
}

function buildTrashMultiContent(paneId: PaneId, platform: PlatformOs): ContextMenuContent {
  const selectedEntries = selectedEntriesForPane(paneId)

  return {
    topStrip: [],
    sections: [
      section('primary', [
        commandRow('restore', platform, {
          strong: true,
          disabled: selectedEntries.some((entry) => !entry.originalPath),
        }),
        commandRow('deletePermanent', platform, { danger: true }),
      ]),
    ],
  }
}

function buildTrashEmptyContent(platform: PlatformOs): ContextMenuContent {
  return {
    topStrip: [],
    sections: [
      section('primary', [commandRow('emptyTrash', platform, { danger: true, strong: true })]),
    ],
  }
}

type EntryTarget = Extract<ContextMenuTarget, { kind: 'file' | 'folder' }>

function entryPrimarySection(target: EntryTarget, platform: PlatformOs): ContextMenuSection {
  const targetEntryId = target.entry.id
  return section('primary', [
    commandRow('open', platform, { targetEntryId, strong: true }),
    commandRow('openInNewTab', platform, {
      targetEntryId,
      disabled: target.kind === 'file',
    }),
    commandRow('openInOtherPane', platform, {
      targetEntryId,
      disabled: target.kind === 'file',
    }),
    customRow({
      id: `open-with-${target.entry.id}`,
      label: 'Open with',
      action: openWithContextAction(target.entry.path),
      disabled: target.kind === 'folder',
      icon: { kind: 'app', name: 'open-with' },
    }),
  ])
}

function entrySecondarySection(
  paneId: PaneId,
  target: EntryTarget,
  platform: PlatformOs,
): ContextMenuSection {
  const canPaste = hasClipboardEntries()
  const panePath = usePanesStore.getState().panes[paneId].path
  const entryPaths = [target.entry.path]
  return section('secondary', [
    ...(target.kind === 'folder' ? [favouriteRow(target.entry.path)] : []),
    commandRow('paste', platform, { disabled: !canPaste }),
    customRow({
      id: `copy-path-${target.entry.id}`,
      label: 'Copy as path',
      action: copyPathsContextAction(entryPaths),
      icon: { kind: 'app', name: 'copy' },
    }),
    customRow({
      id: `compress-${target.entry.id}`,
      label: 'Compress',
      action: compressContextAction(entryPaths, panePath),
      icon: { kind: 'app', name: 'archive' },
    }),
    customRow({
      id: `extract-${target.entry.id}`,
      label: 'Extract',
      action: extractContextAction(entryPaths, panePath),
      disabled: !isZipArchiveEntry(target.entry),
      icon: { kind: 'app', name: 'extract' },
    }),
    ...(target.kind === 'folder' ? [calculateSizeRow(target.entry, platform)] : []),
    commandRow('deletePermanent', platform, { targetEntryId: target.entry.id, danger: true }),
  ])
}

export function buildFileOrFolderContent(
  paneId: PaneId,
  target: EntryTarget,
  platform: PlatformOs,
): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashEntryContent(target, platform)
  }

  const targetItem = toPropertiesDialogItem(target.entry)
  return {
    topStrip: fileSystemStrip(target.entry.id),
    sections: [
      entryPrimarySection(target, platform),
      entrySecondarySection(paneId, target, platform),
      section('footer', [
        propertiesRow({
          id: `properties-${target.entry.id}`,
          items: [targetItem],
        }),
      ]),
    ],
  }
}

function selectionSection(
  paneId: PaneId,
  selectedEntries: DirectoryEntry[],
  platform: PlatformOs,
): ContextMenuSection {
  const pane = usePanesStore.getState().panes[paneId]
  const selectedPaths = selectedEntries.map((entry) => entry.path)
  const nothingSelected = selectedEntries.length === 0
  const canExtract = !nothingSelected && selectedEntries.every(isZipArchiveEntry)
  return section('selection', [
    customRow({
      id: 'compress-selection',
      label: 'Compress',
      action: compressContextAction(selectedPaths, pane.path),
      disabled: nothingSelected,
      icon: { kind: 'app', name: 'archive' },
    }),
    customRow({
      id: 'extract-selection',
      label: 'Extract',
      action: extractContextAction(selectedPaths, pane.path),
      disabled: !canExtract,
      icon: { kind: 'app', name: 'extract' },
    }),
    commandRow('paste', platform, { disabled: !hasClipboardEntries() }),
    customRow({
      id: 'copy-path-selection',
      label: 'Copy as path',
      action: copyPathsContextAction(selectedPaths),
      disabled: nothingSelected,
      icon: { kind: 'app', name: 'copy' },
    }),
    commandRow('deletePermanent', platform, {
      danger: true,
      disabled: nothingSelected,
    }),
    commandRow('selectAll', platform, {
      disabled: pane.entries.length === 0,
    }),
  ])
}

export function buildMultiContent(paneId: PaneId, platform: PlatformOs): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashMultiContent(paneId, platform)
  }

  const selectedEntries = selectedEntriesForPane(paneId)
  const nothingSelected = selectedEntries.length === 0
  return {
    topStrip: fileSystemStrip(undefined, {
      includeRename: false,
      copyDisabled: nothingSelected,
      cutDisabled: nothingSelected,
      deleteDisabled: nothingSelected,
    }),
    sections: [
      selectionSection(paneId, selectedEntries, platform),
      section('footer', [
        propertiesRow({
          id: 'properties-selection',
          items: selectedEntries.map(toPropertiesDialogItem),
          disabled: nothingSelected,
        }),
      ]),
    ],
  }
}

export function buildEmptyContent(paneId: PaneId, platform: PlatformOs): ContextMenuContent {
  if (isTrashPane(paneId)) {
    return buildTrashEmptyContent(platform)
  }

  const pane = usePanesStore.getState().panes[paneId]

  return {
    topStrip: [],
    sections: [
      section('create', [
        commandRow('paste', platform, { disabled: !hasClipboardEntries() }),
        commandRow('newFolder', platform),
        commandRow('newFile', platform),
      ]),
      section('footer', [
        favouriteRow(pane.path),
        commandRow('refresh', platform),
        commandRow('selectAll', platform, {
          disabled: pane.entries.length === 0,
        }),
      ]),
      section('properties', [
        propertiesRow({
          id: `properties-empty-${pane.path}`,
          items: [createPathPropertiesDialogItem(pane.path)],
        }),
      ]),
    ],
  }
}
