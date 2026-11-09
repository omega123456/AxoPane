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
import { isNetworkPath } from '@/lib/volumes'
import { useActionDialogStore, type DeleteTarget } from '@/stores/action-dialog-store'
import { hasClipboardEntries, useClipboardStore } from '@/stores/clipboard-store'
import { useErrorToastStore } from '@/stores/error-toast-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { indexEntriesById, usePanesStore } from '@/stores/panes-store'
import { useSearchPaletteStore } from '@/stores/search-palette-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useSettingsStore } from '@/stores/settings-store'
import { otherPane } from '@/lib/pane-id'
import type { PaneId } from '@/types/pane'

const PARENT_ROW_ID = '..'

const trashPaneCommands = new Set<CommandId>([
  'restore',
  'emptyTrash',
  'delete',
  'deletePermanent',
  'refresh',
  'selectAll',
  'clearFilter',
  'showSettings',
  'searchEverywhere',
])

const trashOnlyCommands = new Set<CommandId>(['restore', 'emptyTrash'])

type PanesSnapshot = ReturnType<typeof usePanesStore.getState>
type PaneSnapshot = PanesSnapshot['panes'][PaneId]

type CommandContext = {
  commandId: CommandId
  paneId: PaneId
  targetEntryId?: string
  panes: PanesSnapshot
  pane: PaneSnapshot
  inTrashPane: boolean
  entriesById: Map<string, DirectoryEntry>
  entry?: DirectoryEntry
  selectedEntries: DirectoryEntry[]
  effectiveEntries: DirectoryEntry[]
  transferEntries: DirectoryEntry[]
}

type CommandHandler = (context: CommandContext) => void

type AvailabilityContext = {
  paneId: PaneId
  pane: PaneSnapshot
  entry?: DirectoryEntry
  hasSelection: boolean
  clipboardHasEntries: boolean
}

type AvailabilityCheck = (context: AvailabilityContext) => boolean

function isCommandAllowedInPane(commandId: CommandId, inTrashPane: boolean): boolean {
  if (inTrashPane) {
    return trashPaneCommands.has(commandId)
  }
  return !trashOnlyCommands.has(commandId)
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error)
}

function toDeleteTargets(entries: DirectoryEntry[]): DeleteTarget[] {
  return entries.map((item) => ({
    id: item.id,
    name: item.name,
    path: item.path,
    sizeBytes: item.sizeBytes,
  }))
}

function entryPaths(entries: DirectoryEntry[]): string[] {
  return entries.map((item) => item.path)
}

function transferEntriesFor(
  entry: DirectoryEntry | undefined,
  selectedEntries: DirectoryEntry[],
  focusedEntry: DirectoryEntry | undefined,
): DirectoryEntry[] {
  if (entry) {
    return [entry]
  }
  if (selectedEntries.length > 0) {
    return selectedEntries
  }
  return focusedEntry ? [focusedEntry] : []
}

function focusedEntryIdFor(context: CommandContext): string | null {
  return context.targetEntryId ?? context.pane.focusedEntryId
}

function openEntry(context: CommandContext, focused: DirectoryEntry) {
  if (focused.isDir) {
    void context.panes.navigatePane(context.paneId, focused.path)
    return
  }
  void openPath({ path: focused.path }).catch((error) => {
    log.error('open command failed', { paneId: context.paneId, path: focused.path, error })
  })
}

function openFocused(context: CommandContext) {
  const focusId = focusedEntryIdFor(context)
  if (focusId === PARENT_ROW_ID) {
    void context.panes.goUp(context.paneId)
    return
  }
  const focused = focusId ? context.entriesById.get(focusId) : undefined
  if (focused) {
    openEntry(context, focused)
  }
}

function calculateFocusedSize(context: CommandContext) {
  const focusId = focusedEntryIdFor(context)
  if (focusId) {
    void context.panes.requestManualSize(context.paneId, focusId)
  }
}

function openDirectoryInPane(context: CommandContext, destinationPaneId: PaneId) {
  if (context.entry?.isDir) {
    void context.panes.openTabFromPath(destinationPaneId, context.entry.path)
  }
}

function selectAllEntries({ pane, paneId }: CommandContext) {
  const firstEntryId = pane.entries[0]?.id ?? null
  useSelectionStore.getState().setSelection(paneId, {
    selectedIds: pane.entries.map((item) => item.id),
    anchorId: firstEntryId,
    focusedId: firstEntryId,
  })
}

function soleSelectedEntry(selectedEntries: DirectoryEntry[]): DirectoryEntry | undefined {
  return selectedEntries.length === 1 ? selectedEntries[0] : undefined
}

function beginRename({ entry, selectedEntries, paneId }: CommandContext) {
  const target = entry ?? soleSelectedEntry(selectedEntries)
  if (!target) {
    return
  }
  useInlineRenameStore.getState().beginRename({
    paneId,
    entryId: target.id,
    path: target.path,
    initialValue: target.name,
  })
}

function confirmPermanentDelete({ inTrashPane, paneId, effectiveEntries }: CommandContext) {
  useActionDialogStore.getState().open({
    kind: inTrashPane ? 'deleteFromTrash' : 'delete',
    paneId,
    targets: toDeleteTargets(effectiveEntries),
  })
}

function moveEntriesToTrash({ paneId, effectiveEntries }: CommandContext) {
  const paths = entryPaths(effectiveEntries)
  void moveToTrash({ paths }).catch((error) => {
    log.error('move to trash failed', { paneId, paths, error })
    useErrorToastStore.getState().show(errorMessage(error))
  })
}

function hasEffectiveEntries(context: CommandContext) {
  return context.effectiveEntries.length > 0
}

function deleteEntries(context: CommandContext) {
  if (!hasEffectiveEntries(context)) {
    return
  }
  const onNetworkVolume = context.effectiveEntries.some((item) =>
    isNetworkPath(item.path, context.panes.volumes),
  )
  const needsConfirmation = context.inTrashPane || onNetworkVolume
  if (needsConfirmation) {
    confirmPermanentDelete(context)
    return
  }
  moveEntriesToTrash(context)
}

function deleteEntriesPermanently(context: CommandContext) {
  if (hasEffectiveEntries(context)) {
    confirmPermanentDelete(context)
  }
}

function restoreEntries({ effectiveEntries, panes, paneId }: CommandContext) {
  const trashIds = effectiveEntries
    .map((item) => item.trashId)
    .filter((id): id is string => Boolean(id))
  if (trashIds.length === 0) {
    return
  }
  void restoreFromTrash({ ids: trashIds })
    .then(() => panes.reloadPane(paneId))
    .catch((error) => {
      log.error('restore from trash failed', { paneId, trashIds, error })
      useErrorToastStore.getState().show(errorMessage(error))
    })
}

function confirmEmptyTrash({ pane, paneId }: CommandContext) {
  const count = pane.entries.length
  if (count > 0) {
    useActionDialogStore.getState().open({ kind: 'emptyTrash', paneId, count })
  }
}

function placeOnClipboard(mode: 'copy' | 'move'): CommandHandler {
  return ({ effectiveEntries, paneId }) => {
    if (effectiveEntries.length === 0) {
      return
    }
    useClipboardStore
      .getState()
      .setClipboard({ mode, sourcePaneId: paneId, entries: effectiveEntries })
    const paths = entryPaths(effectiveEntries)
    void writeFileClipboard({ mode, paths }).catch((error) => {
      log.warn('write file clipboard failed', { paneId, mode, paths, error })
    })
  }
}

function clearClipboardAfterMove(paneId: PaneId) {
  useClipboardStore.getState().clearClipboard()
  void clearFileClipboard().catch((error) => {
    log.warn('clear file clipboard failed', { paneId, error })
  })
}

function pasteClipboard({ pane, paneId }: CommandContext) {
  const clipboard = useClipboardStore.getState().snapshot
  if (!clipboard) {
    return
  }
  const { mode, entries } = clipboard
  if (entries.length === 0) {
    return
  }
  void startOp({
    kind: mode,
    destinationDir: pane.path,
    items: entries.map((item) => ({
      sourcePath: item.path,
      name: item.name,
      sizeBytes: item.sizeBytes ?? 0,
    })),
  }).then(() => {
    if (mode === 'move') {
      clearClipboardAfterMove(paneId)
    }
  })
}

function confirmTransferToOtherPane(operation: 'copy' | 'move'): CommandHandler {
  return ({ transferEntries, panes, pane, paneId }) => {
    if (transferEntries.length === 0) {
      return
    }
    useActionDialogStore.getState().open({
      kind: 'transferConfirm',
      paneId,
      operation,
      sourceDir: pane.path,
      destinationDir: panes.panes[otherPane(paneId)].path,
      targets: toDeleteTargets(transferEntries),
    })
  }
}

function openActionDialog(kind: 'newFolder' | 'newFile'): CommandHandler {
  return ({ paneId }) => useActionDialogStore.getState().open({ kind, paneId })
}

const commandHandlers: Record<CommandId, CommandHandler> = {
  open: openFocused,
  goUp: ({ panes, paneId }) => void panes.goUp(paneId),
  refresh: ({ panes, paneId }) => void panes.refreshSizes(paneId),
  calculateSize: calculateFocusedSize,
  openInNewTab: (context) => openDirectoryInPane(context, context.paneId),
  openInOtherPane: (context) => openDirectoryInPane(context, otherPane(context.paneId)),
  selectAll: selectAllEntries,
  clearFilter: ({ panes, paneId }) => panes.clearFilter(paneId),
  showSettings: () => useSettingsStore.getState().open(),
  searchEverywhere: () => useSearchPaletteStore.getState().open(),
  newFolder: openActionDialog('newFolder'),
  newFile: openActionDialog('newFile'),
  rename: beginRename,
  delete: deleteEntries,
  deletePermanent: deleteEntriesPermanently,
  restore: restoreEntries,
  emptyTrash: confirmEmptyTrash,
  copy: placeOnClipboard('copy'),
  cut: placeOnClipboard('move'),
  paste: pasteClipboard,
  copyToOtherPane: confirmTransferToOtherPane('copy'),
  moveToOtherPane: confirmTransferToOtherPane('move'),
}

function buildCommandContext(
  commandId: CommandId,
  paneId: PaneId,
  targetEntryId: string | undefined,
): CommandContext {
  const panes = usePanesStore.getState()
  const pane = panes.panes[paneId]
  const entriesById = indexEntriesById(pane.entries)
  const entry = targetEntryId ? entriesById.get(targetEntryId) : undefined
  const selectedEntries = selectedEntriesForPane(paneId)
  const focusedEntry = pane.focusedEntryId ? entriesById.get(pane.focusedEntryId) : undefined
  return {
    commandId,
    paneId,
    targetEntryId,
    panes,
    pane,
    inTrashPane: isTrashPath(pane.path),
    entriesById,
    entry,
    selectedEntries,
    effectiveEntries: entry ? [entry] : selectedEntries,
    transferEntries: transferEntriesFor(entry, selectedEntries, focusedEntry),
  }
}

export function executeCommand(commandId: CommandId, paneId: PaneId, targetEntryId?: string) {
  const context = buildCommandContext(commandId, paneId, targetEntryId)
  if (!isCommandAllowedInPane(commandId, context.inTrashPane)) {
    return
  }
  const handler = commandHandlers[commandId] as CommandHandler | undefined
  if (!handler) {
    log.info('command not implemented', {
      commandId,
      paneId,
      targetEntryId,
      entries: context.effectiveEntries.length,
    })
    return
  }
  handler(context)
}

const hasTargetOrSelection: AvailabilityCheck = ({ entry, hasSelection }) =>
  Boolean(entry) || hasSelection

const targetIsDirectory: AvailabilityCheck = ({ entry }) => Boolean(entry?.isDir)

const paneHasEntries: AvailabilityCheck = ({ pane }) => pane.entries.length > 0

function canRestore({ entry, paneId }: AvailabilityContext): boolean {
  const effectiveEntries = entry ? [entry] : selectedEntriesForPane(paneId)
  if (effectiveEntries.length === 0) {
    return false
  }
  return effectiveEntries.every((item) => item.originalPath)
}

const canTransferToOtherPane: AvailabilityCheck = (context) =>
  hasTargetOrSelection(context) || Boolean(context.pane.focusedEntryId)

const availabilityChecks: Partial<Record<CommandId, AvailabilityCheck>> = {
  paste: ({ clipboardHasEntries }) => clipboardHasEntries,
  selectAll: paneHasEntries,
  openInNewTab: targetIsDirectory,
  openInOtherPane: targetIsDirectory,
  calculateSize: targetIsDirectory,
  copy: hasTargetOrSelection,
  cut: hasTargetOrSelection,
  rename: hasTargetOrSelection,
  delete: hasTargetOrSelection,
  deletePermanent: hasTargetOrSelection,
  restore: canRestore,
  emptyTrash: paneHasEntries,
  copyToOtherPane: canTransferToOtherPane,
  moveToOtherPane: canTransferToOtherPane,
}

const alwaysAvailable: AvailabilityCheck = () => true

export function canExecuteCommand(commandId: CommandId, paneId: PaneId, targetEntryId?: string) {
  const pane = usePanesStore.getState().panes[paneId]
  if (!isCommandAllowedInPane(commandId, isTrashPath(pane.path))) {
    return false
  }
  const check = availabilityChecks[commandId] ?? alwaysAvailable
  return check({
    paneId,
    pane,
    entry: targetEntryId ? indexEntriesById(pane.entries).get(targetEntryId) : undefined,
    hasSelection: useSelectionStore.getState().selections[paneId].selectedIds.length > 0,
    clipboardHasEntries: hasClipboardEntries(),
  })
}

export function selectedEntriesForPane(paneId: PaneId): DirectoryEntry[] {
  const pane = usePanesStore.getState().panes[paneId]
  const selection = useSelectionStore.getState().selections[paneId]
  const selectedIdSet = new Set(selection.selectedIds)
  return pane.entries.filter((entry) => selectedIdSet.has(entry.id))
}
