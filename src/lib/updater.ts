import { log } from '@/lib/app-log-commands'

export type UpdateSummary = {
  currentVersion: string
  version: string
  notes?: string
  date?: string
}

// A minimal structural view of the `@tauri-apps/plugin-updater` `Update` handle.
// Typed locally so this module (and its tests) do not require the native plugin
// to be present at type-check time.
export type AppUpdate = {
  currentVersion: string
  version: string
  body?: string
  date?: string
  downloadAndInstall: () => Promise<void>
}

export function summarizeUpdate(update: AppUpdate): UpdateSummary {
  return {
    currentVersion: update.currentVersion,
    version: update.version,
    notes: update.body,
    date: update.date,
  }
}

function canUseNativeUpdater() {
  if (import.meta.env.VITE_PLAYWRIGHT) {
    return false
  }

  return (
    typeof window !== 'undefined' &&
    '__TAURI_INTERNALS__' in window &&
    navigator.userAgent.includes('Windows')
  )
}

export async function checkForAppUpdate(): Promise<AppUpdate | null> {
  if (!canUseNativeUpdater()) {
    return null
  }

  const { check } = await import('@tauri-apps/plugin-updater')
  const update = (await check()) as AppUpdate | null
  if (!update) {
    return null
  }

  log.info('app update available', summarizeUpdate(update))
  return update
}

/**
 * Downloads and applies an update, then relaunches the app. Accepts an already
 * fetched handle (from {@link checkForAppUpdate}) so the prompt flow does not
 * re-query the endpoint; falls back to checking when called without one.
 */
export async function downloadAndInstallAppUpdate(update?: AppUpdate | null): Promise<boolean> {
  const target = update ?? (await checkForAppUpdate())
  if (!target) {
    return false
  }

  await target.downloadAndInstall()
  log.info('app update downloaded and installed', {
    currentVersion: target.currentVersion,
    version: target.version,
  })

  // On Windows the NSIS passive installer (see `tauri.conf.json` →
  // plugins.updater.windows.installMode) restarts the app after applying the
  // update, so no explicit relaunch is needed here.
  //
  // TODO(release): for an explicit cross-platform relaunch, add the
  // `@tauri-apps/plugin-process` dependency + `tauri-plugin-process` crate +
  // `process:default` capability, then call its `relaunch()` here.
  return true
}
