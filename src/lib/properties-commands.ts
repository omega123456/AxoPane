import { log } from '@/lib/app-log-commands'
import { showNativeProperties } from '@/lib/context-menu/native-menu-commands'
import { TRASH_PATH } from '@/lib/trash'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import type { DirectoryEntry } from '@/lib/types/ipc'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'
import type { PlatformOs } from '@/lib/keymap'

// Windows resolves this well-known Explorer namespace URI to the real
// Recycle Bin shell item, letting the native Properties COM flow work even
// though the app's trash browser has no real filesystem path backing it.
const WINDOWS_RECYCLE_BIN_SHELL_PATH = 'shell:RecycleBinFolder'

function shouldAttemptNativeProperties(items: PropertiesDialogItem[]) {
  return items.length > 0 && items.every((item) => item.path.trim().length > 0)
}

function describePropertiesResult(items: PropertiesDialogItem[]) {
  return items.length === 1
    ? { count: 1, path: items[0]?.path }
    : { count: items.length, paths: items.map((item) => item.path) }
}

export function toPropertiesDialogItem(entry: DirectoryEntry): PropertiesDialogItem {
  return {
    attributes: entry.attributes,
    createdAt: entry.createdAt,
    id: entry.id,
    isDir: entry.isDir,
    isHidden: entry.isHidden,
    isSystem: entry.isSystem,
    itemCount: entry.itemCount,
    modifiedAt: entry.modifiedAt,
    name: entry.name,
    path: entry.path,
    sizeBytes: entry.sizeBytes,
    typeLabel: entry.typeLabel,
  }
}

export function createPathPropertiesDialogItem(path: string): PropertiesDialogItem {
  const normalized = path.replace(/[\\/]+$/, '')
  const parts = normalized.split(/[\\/]/).filter(Boolean)
  const name = parts.at(-1) ?? normalized

  return {
    attributes: [],
    createdAt: null,
    id: path,
    isDir: true,
    isHidden: false,
    isSystem: false,
    itemCount: null,
    modifiedAt: null,
    name,
    path,
    sizeBytes: null,
    typeLabel: 'Folder',
  }
}

export function createTrashPropertiesDialogItem(os: PlatformOs): PropertiesDialogItem {
  return {
    attributes: [],
    createdAt: null,
    id: TRASH_PATH,
    isDir: true,
    isHidden: false,
    isSystem: true,
    itemCount: null,
    modifiedAt: null,
    name: 'Recycle Bin',
    path: os === 'windows' ? WINDOWS_RECYCLE_BIN_SHELL_PATH : TRASH_PATH,
    sizeBytes: null,
    typeLabel: 'System Folder',
  }
}

export async function showPropertiesDialog(items: PropertiesDialogItem[]) {
  if (items.length === 0) {
    log.info('properties requested without any targets')
    return
  }

  if (shouldAttemptNativeProperties(items)) {
    try {
      const response = await showNativeProperties({ paths: items.map((item) => item.path) })
      if (response.handled) {
        return
      }

      log.info('native properties unavailable, opening fallback dialog', {
        ...describePropertiesResult(items),
        message: response.message ?? null,
      })
    } catch (error) {
      log.warn('show_properties IPC failed, opening fallback dialog', {
        ...describePropertiesResult(items),
        error,
      })
    }
  }

  log.info('opening fallback properties dialog', describePropertiesResult(items))
  usePropertiesDialogStore.getState().open({ items })
}
