import { log } from '@/lib/app-log-commands'
import { detectPlatformOs } from '@/lib/keymap'
import {
  getDefaultApplication as getDefaultApplicationIpc,
  listApplications as listApplicationsIpc,
  setDefaultApplication as setDefaultApplicationIpc,
} from '@/lib/ipc/commands'
import type { MacApp } from '@/lib/types/ipc'
import type { PropertiesDialogItem } from '@/stores/properties-dialog-store'

/**
 * "Set Default Application…" is macOS-only (Windows already covers this via
 * the native Open With dialog's "always use" checkbox) and only meaningful
 * for a single, non-folder selection that actually has a file extension to
 * key the association on.
 */
export function canSetDefaultApplication(items: PropertiesDialogItem[]): boolean {
  if (detectPlatformOs() !== 'macos' || items.length !== 1) {
    return false
  }

  const [item] = items
  return !item.isDir && item.name.lastIndexOf('.') > 0
}

export async function getDefaultApplication(filePath: string): Promise<MacApp | null> {
  try {
    const response = await getDefaultApplicationIpc({ path: filePath })
    return response.app
  } catch (error) {
    log.warn('get_default_application IPC failed', { path: filePath, error })
    return null
  }
}

export async function listApplications(): Promise<MacApp[]> {
  try {
    const response = await listApplicationsIpc()
    return response.apps
  } catch (error) {
    log.warn('list_applications IPC failed', { error })
    return []
  }
}

export async function setDefaultApplication(filePath: string, app: MacApp): Promise<boolean> {
  try {
    const response = await setDefaultApplicationIpc({ path: filePath, bundlePath: app.bundlePath })
    if (!response.handled) {
      log.info('set_default_application reported unhandled', {
        path: filePath,
        bundlePath: app.bundlePath,
        message: response.message ?? null,
      })
    }
    return response.handled
  } catch (error) {
    log.warn('set_default_application IPC failed', {
      path: filePath,
      bundlePath: app.bundlePath,
      error,
    })
    return false
  }
}
