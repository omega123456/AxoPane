import { log } from '@/lib/app-log-commands'
import { executeCommand } from '@/lib/commands'
import {
  invokeNativeMenu,
  showNativeOpenWith,
} from '@/lib/context-menu/native-menu-commands'
import { requestFolderSize } from '@/lib/ipc/commands'
import { showPropertiesDialog } from '@/lib/properties-commands'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'
import type { CommandId } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'

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
      kind: 'close-tab'
      tabId: string
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

export function closeTabContextAction(tabId: string): ContextMenuAction {
  return { kind: 'close-tab', tabId }
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

export function compressContextAction(
  paths: string[],
  destinationDir: string,
): ContextMenuAction {
  return { kind: 'compress', paths, destinationDir }
}

export function extractContextAction(paths: string[], destinationDir: string): ContextMenuAction {
  return { kind: 'extract', paths, destinationDir }
}

export function shareContextAction(paths: string[]): ContextMenuAction {
  return { kind: 'share', paths }
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

export function dispatchContextMenuAction(paneId: PaneId, action: ContextMenuAction) {
  switch (action.kind) {
    case 'command':
      executeCommand(action.commandId, paneId, action.targetEntryId)
      return
    case 'navigate': {
      const destinationPaneId = action.destination === 'other-pane'
        ? paneId === 'left'
          ? 'right'
          : 'left'
        : paneId
      void usePanesStore.getState().navigatePane(destinationPaneId, action.path)
      return
    }
    case 'open-path-in-new-tab':
      void usePanesStore.getState().openTabFromPath(paneId, action.path)
      return
    case 'close-tab':
      void usePanesStore.getState().closeTab(paneId, action.tabId)
      return
    case 'request-folder-size':
      void requestFolderSize({ path: action.path })
      return
    case 'open-with':
      void showNativeOpenWith({ path: action.path })
        .then((response) => {
          if (!response.handled) {
            log.warn('open_with unavailable', {
              path: action.path,
              message: response.message ?? null,
            })
          }
        })
        .catch((error) => {
          log.warn('open_with IPC failed', { path: action.path, error })
        })
      return
    case 'properties':
      void showPropertiesDialog(action.items)
      return
    case 'compress':
      useActionDialogStore.getState().open({
        kind: 'archiveConfirm',
        paneId,
        operation: 'compress',
        destinationDir: action.destinationDir,
        targets: action.paths.map((path) => ({
          id: path,
          name: pathName(path),
          path,
        })),
      })
      return
    case 'extract':
      useActionDialogStore.getState().open({
        kind: 'archiveConfirm',
        paneId,
        operation: 'extract',
        destinationDir: action.destinationDir,
        targets: action.paths.map((path) => ({
          id: path,
          name: pathName(path),
          path,
          sizeBytes: 0,
        })),
      })
      return
    case 'share':
      log.info('share command unavailable', {
        path: action.paths.length === 1 ? action.paths[0] : undefined,
        paths: action.paths.length > 1 ? action.paths : undefined,
      })
      return
    case 'invoke-native':
      void invokeNativeMenu({ token: action.token })
        .then((response) => {
          if (!response.handled) {
            log.info('native menu command unavailable', {
              token: action.token,
              message: response.message ?? null,
            })
          }
        })
        .catch((error) => {
          log.warn('invoke_native_menu_action IPC failed', { token: action.token, error })
        })
      return
    case 'noop':
      return
  }
}
