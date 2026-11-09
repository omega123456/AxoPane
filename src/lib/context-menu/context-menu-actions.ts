import { log } from '@/lib/app-log-commands'
import { executeCommand } from '@/lib/commands'
import { runEjectVolume } from '@/lib/eject-commands'
import { copyPathsToClipboard } from '@/lib/path-clipboard'
import { invokeNativeMenu, showNativeOpenWith } from '@/lib/context-menu/native-menu-commands'
import { requestFolderSize } from '@/lib/ipc/commands'
import { showPropertiesDialog } from '@/lib/properties-commands'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useErrorToastStore } from '@/stores/error-toast-store'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'
import type { CommandId } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { useConfigStore } from '@/stores/config-store'
import type { PaneId } from '@/types/pane'
import { otherPane } from '@/lib/pane-id'
import { dispatchByKind, type KindHandlers } from '@/lib/kind-table'

export type ContextMenuAction =
  | {
      kind: 'command'
      commandId: CommandId
      targetEntryId?: string
    }
  | {
      kind: 'navigate'
      path: string
      destination: 'current-pane' | 'other-pane'
    }
  | {
      kind: 'open-path-in-new-tab'
      path: string
    }
  | {
      kind: 'open-path-in-explicit-pane'
      path: string
      paneId: PaneId
    }
  | {
      kind: 'close-tab'
      tabId: string
    }
  | {
      kind: 'close-tabs-beside'
      tabId: string
      side: 'left' | 'right'
    }
  | {
      kind: 'set-tab-locked'
      tabId: string
      locked: boolean
    }
  | {
      kind: 'move-tab'
      tabId: string
      destinationPaneId: PaneId
      destinationIndex: number
    }
  | { kind: 'add-favourite'; path: string }
  | { kind: 'remove-favourite'; path: string }
  | { kind: 'rename-favourite'; path: string }
  | {
      kind: 'copy-paths'
      paths: string[]
    }
  | {
      kind: 'request-folder-size'
      path: string
    }
  | {
      kind: 'open-with'
      path: string
    }
  | {
      kind: 'properties'
      items: PropertiesDialogItem[]
    }
  | {
      kind: 'compress'
      paths: string[]
      destinationDir: string
    }
  | {
      kind: 'extract'
      paths: string[]
      destinationDir: string
    }
  | {
      kind: 'share'
      paths: string[]
    }
  | {
      kind: 'eject-volume'
      mountRoot: string
    }
  | {
      kind: 'invoke-native'
      token: string
    }
  | {
      kind: 'noop'
      debugId: string
    }

export function commandContextAction(
  commandId: CommandId,
  targetEntryId?: string,
): ContextMenuAction {
  return { kind: 'command', commandId, targetEntryId }
}

export function navigateContextAction(
  path: string,
  destination: 'current-pane' | 'other-pane',
): ContextMenuAction {
  return { kind: 'navigate', path, destination }
}

export function openPathInNewTabContextAction(path: string): ContextMenuAction {
  return { kind: 'open-path-in-new-tab', path }
}

export function openPathInExplicitPaneContextAction(
  path: string,
  paneId: PaneId,
): ContextMenuAction {
  return { kind: 'open-path-in-explicit-pane', path, paneId }
}

export function closeTabContextAction(tabId: string): ContextMenuAction {
  return { kind: 'close-tab', tabId }
}

export function closeTabsBesideContextAction(
  tabId: string,
  side: 'left' | 'right',
): ContextMenuAction {
  return { kind: 'close-tabs-beside', tabId, side }
}

export function setTabLockedContextAction(tabId: string, locked: boolean): ContextMenuAction {
  return { kind: 'set-tab-locked', tabId, locked }
}

export function moveTabContextAction(
  tabId: string,
  destinationPaneId: PaneId,
  destinationIndex: number,
): ContextMenuAction {
  return { kind: 'move-tab', tabId, destinationPaneId, destinationIndex }
}

export function addFavouriteContextAction(path: string): ContextMenuAction {
  return { kind: 'add-favourite', path }
}

export function removeFavouriteContextAction(path: string): ContextMenuAction {
  return { kind: 'remove-favourite', path }
}

export function renameFavouriteContextAction(path: string): ContextMenuAction {
  return { kind: 'rename-favourite', path }
}

export function copyPathsContextAction(paths: string[]): ContextMenuAction {
  return { kind: 'copy-paths', paths }
}

export function requestFolderSizeContextAction(path: string): ContextMenuAction {
  return { kind: 'request-folder-size', path }
}

export function openWithContextAction(path: string): ContextMenuAction {
  return { kind: 'open-with', path }
}

export function propertiesContextAction(items: PropertiesDialogItem[]): ContextMenuAction {
  return { kind: 'properties', items }
}

export function compressContextAction(paths: string[], destinationDir: string): ContextMenuAction {
  return { kind: 'compress', paths, destinationDir }
}

export function extractContextAction(paths: string[], destinationDir: string): ContextMenuAction {
  return { kind: 'extract', paths, destinationDir }
}

export function shareContextAction(paths: string[]): ContextMenuAction {
  return { kind: 'share', paths }
}

export function ejectVolumeContextAction(mountRoot: string): ContextMenuAction {
  return { kind: 'eject-volume', mountRoot }
}

export function nativeInvocationContextAction(token: string): ContextMenuAction {
  return { kind: 'invoke-native', token }
}

export function noopContextAction(debugId: string): ContextMenuAction {
  return { kind: 'noop', debugId }
}

function pathName(path: string) {
  const normalized = path.replace(/\\/g, '/')
  return normalized.split('/').filter(Boolean).pop() ?? path
}

type ContextMenuActionOfKind<Kind extends ContextMenuAction['kind']> = Extract<
  ContextMenuAction,
  { kind: Kind }
>

function archiveTargets(paths: string[]) {
  return paths.map((path) => ({ id: path, name: pathName(path), path }))
}

function navigate(action: ContextMenuActionOfKind<'navigate'>, paneId: PaneId) {
  if (action.destination === 'other-pane') {
    void usePanesStore.getState().openTabFromPath(otherPane(paneId), action.path)
    return
  }
  void usePanesStore.getState().navigatePane(paneId, action.path)
}

function openWith(path: string) {
  void showNativeOpenWith({ path })
    .then((response) => {
      if (!response.handled) {
        log.warn('open_with unavailable', { path, message: response.message ?? null })
      }
    })
    .catch((error) => {
      log.warn('open_with IPC failed', { path, error })
    })
}

function logShareUnavailable(paths: string[]) {
  log.info('share command unavailable', {
    path: paths.length === 1 ? paths[0] : undefined,
    paths: paths.length > 1 ? paths : undefined,
  })
}

function ejectVolume(mountRoot: string) {
  void runEjectVolume({ mountRoot }).then((response) => {
    if (!response.handled) {
      useErrorToastStore.getState().show(response.message ?? 'Could not eject the drive.')
    }
  })
}

function invokeNative(token: string) {
  void invokeNativeMenu({ token })
    .then((response) => {
      if (!response.handled) {
        log.info('native menu command unavailable', { token, message: response.message ?? null })
      }
    })
    .catch((error) => {
      log.warn('invoke_native_menu_action IPC failed', { token, error })
    })
}

const contextMenuActionHandlers: KindHandlers<ContextMenuAction, [paneId: PaneId], void> = {
  command: (action, paneId) => executeCommand(action.commandId, paneId, action.targetEntryId),
  navigate,
  'open-path-in-new-tab': (action, paneId) =>
    void usePanesStore.getState().openTabFromPath(paneId, action.path),
  'open-path-in-explicit-pane': (action) =>
    void usePanesStore.getState().openTabFromPath(action.paneId, action.path),
  'close-tab': (action, paneId) => void usePanesStore.getState().closeTab(paneId, action.tabId),
  'close-tabs-beside': (action, paneId) =>
    void usePanesStore.getState().closeTabsBeside(paneId, action.tabId, action.side),
  'set-tab-locked': (action, paneId) =>
    usePanesStore.getState().setTabLocked(paneId, action.tabId, action.locked),
  'move-tab': (action, paneId) =>
    void usePanesStore.getState().moveTab({
      sourcePaneId: paneId,
      tabId: action.tabId,
      destinationPaneId: action.destinationPaneId,
      destinationIndex: action.destinationIndex,
    }),
  'add-favourite': (action) => void useConfigStore.getState().addFavourite(action.path),
  'remove-favourite': (action) => void useConfigStore.getState().removeFavourite(action.path),
  'rename-favourite': (action) => useConfigStore.getState().setRenamingFavourite(action.path),
  'copy-paths': (action) => void copyPathsToClipboard(action.paths),
  'request-folder-size': (action) => void requestFolderSize({ path: action.path }),
  'open-with': (action) => openWith(action.path),
  properties: (action) => void showPropertiesDialog(action.items),
  compress: (action, paneId) =>
    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId,
      operation: 'compress',
      destinationDir: action.destinationDir,
      targets: archiveTargets(action.paths),
    }),
  extract: (action, paneId) =>
    useActionDialogStore.getState().open({
      kind: 'archiveConfirm',
      paneId,
      operation: 'extract',
      destinationDir: action.destinationDir,
      targets: archiveTargets(action.paths).map((target) => ({ ...target, sizeBytes: 0 })),
    }),
  share: (action) => logShareUnavailable(action.paths),
  'eject-volume': (action) => ejectVolume(action.mountRoot),
  'invoke-native': (action) => invokeNative(action.token),
  noop: () => undefined,
}

export function dispatchContextMenuAction(paneId: PaneId, action: ContextMenuAction) {
  dispatchByKind(contextMenuActionHandlers, action, paneId)
}
