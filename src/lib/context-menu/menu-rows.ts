import {
  commandContextAction,
  addFavouriteContextAction,
  propertiesContextAction,
  removeFavouriteContextAction,
} from '@/lib/context-menu/context-menu-actions'
import { getFileCategory } from '@/lib/file-type'
import { isTrashPath } from '@/lib/trash'
import type {
  ContextMenuActionRow,
  ContextMenuIcon,
  ContextMenuRowItem,
  ContextMenuSection,
  ContextMenuStripItem,
} from '@/lib/types/context-menu'
import { commandLabels, formatShortcutLabel, type PlatformOs } from '@/lib/keymap'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useConfigStore } from '@/stores/config-store'
import type { CommandId, DirectoryEntry } from '@/lib/types/ipc'
import type { PaneId } from '@/types/pane'
import { pathsMatch } from '@/lib/path-compare'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'

export function favouriteRow(path: string, remove = false): ContextMenuActionRow {
  const isFavourite = useConfigStore.getState().favourites.some((item) => pathsMatch(item, path))
  return customRow({
    id: `${remove ? 'remove' : 'add'}-favourite-${path}`,
    label: remove ? 'Remove from Favourites' : 'Add to Favourites',
    action: remove ? removeFavouriteContextAction(path) : addFavouriteContextAction(path),
    disabled: !remove && isFavourite,
    icon: { kind: 'app', name: remove ? 'close-tab' : 'new-folder' },
  })
}

export function shortcutFor(commandId: CommandId, platform: PlatformOs) {
  const binding = useKeymapStore.getState().bindings[commandId][0]
  return binding ? formatShortcutLabel(binding, platform) : undefined
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

export function commandRow(
  commandId: CommandId,
  platform: PlatformOs,
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
    shortcut: shortcutFor(commandId, platform),
    disabled: options?.disabled,
    danger: options?.danger,
    hidden: options?.hidden,
    strong: options?.strong,
    action: commandContextAction(commandId, options?.targetEntryId),
  }
}

export type CustomRowSpec = {
  id: string
  label: string
  action: ContextMenuActionRow['action']
  disabled?: boolean
  danger?: boolean
  hidden?: boolean
  strong?: boolean
  shortcut?: string
  icon?: ContextMenuIcon
}

export function customRow(spec: CustomRowSpec): ContextMenuActionRow {
  return {
    id: spec.id,
    kind: 'action',
    label: spec.label,
    owner: 'app',
    disabled: spec.disabled,
    danger: spec.danger,
    hidden: spec.hidden,
    strong: spec.strong,
    shortcut: spec.shortcut,
    icon: spec.icon,
    action: spec.action,
  }
}

const PROPERTIES_LABEL = 'Properties'

const PROPERTIES_ICON: ContextMenuIcon = { kind: 'app', name: 'properties' }

export type PropertiesRowSpec = {
  id: string
  items: PropertiesDialogItem[]
  disabled?: boolean
}

export function propertiesRow(spec: PropertiesRowSpec): ContextMenuActionRow {
  return customRow({
    id: spec.id,
    label: PROPERTIES_LABEL,
    action: propertiesContextAction(spec.items),
    disabled: spec.disabled,
    icon: PROPERTIES_ICON,
  })
}

export function section(id: string, rows: ContextMenuRowItem[]): ContextMenuSection {
  return { id, rows }
}

export function fileSystemStrip(
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

/**
 * "Calculate size" shows when automatic sizing is unavailable or off, or when
 * the folder's size is `unknown`. A folder with no size state yet
 * (e.g. a pane over the auto-size cap, or a tree node) counts as `unknown`.
 */
export function calculateSizeHidden(path: string) {
  const { folderSizeIndexStatus, sizeStates } = usePanesStore.getState()
  return (
    (folderSizeIndexStatus?.isAvailable ?? false) &&
    useConfigStore.getState().autoFolderSize &&
    (sizeStates[path]?.state ?? 'unknown') !== 'unknown'
  )
}

export function calculateSizeRow(
  entry: DirectoryEntry,
  platform: PlatformOs,
): ContextMenuActionRow {
  const volumes = usePanesStore.getState().volumes
  const volume = volumes.find((item) =>
    entry.path.toLowerCase().startsWith(item.mountRoot.toLowerCase()),
  )

  return commandRow('calculateSize', platform, {
    targetEntryId: entry.id,
    hidden: calculateSizeHidden(entry.path),
    disabled: Boolean(volume?.isNetwork),
  })
}

function isArchiveEntry(entry: DirectoryEntry) {
  return getFileCategory(entry) === 'archive'
}

export function isZipArchiveEntry(entry: DirectoryEntry) {
  if (entry.isDir) {
    return false
  }
  if (!isArchiveEntry(entry)) {
    return false
  }

  return entry.name.toLowerCase().endsWith('.zip')
}

export function isTrashPane(paneId: PaneId): boolean {
  return isTrashPath(usePanesStore.getState().panes[paneId].path)
}
