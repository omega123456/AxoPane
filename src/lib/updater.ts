import { log } from '@/lib/app-log-commands'

const PLACEHOLDER_APP_VERSION = '0.1.0'

export type UpdateSummary = {
  currentVersion: string
  version: string
  notes?: string
  date?: string
}

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

function isTauriRuntime() {
  if (import.meta.env.VITE_PLAYWRIGHT) {
    return false
  }

  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function isWindowsRuntime() {
  return typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows')
}

function needsManualRelaunch() {
  return isTauriRuntime() && !isWindowsRuntime()
}

function canUseNativeUpdater() {
  return isTauriRuntime()
}

export async function getAppVersion(): Promise<string> {
  if (!canUseNativeUpdater()) {
    if (import.meta.env.VITE_PLAYWRIGHT) {
      return PLACEHOLDER_APP_VERSION
    }
    return import.meta.env.VITE_APP_VERSION ?? PLACEHOLDER_APP_VERSION
  }

  const { getVersion } = await import('@tauri-apps/api/app')
  return getVersion()
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

  if (needsManualRelaunch()) {
    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  }

  return true
}
