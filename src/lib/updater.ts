import { log } from '@/lib/app-log-commands'

// Fixed, version-bump-proof app version surfaced in Playwright visual tests.
const MOCK_APP_VERSION = '0.1.0'

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

function isTauriRuntime() {
  if (import.meta.env.VITE_PLAYWRIGHT) {
    return false
  }

  return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window
}

function isWindowsRuntime() {
  return typeof navigator !== 'undefined' && navigator.userAgent.includes('Windows')
}

// The native updater works on every desktop target (Windows, macOS, Linux), so
// availability hinges only on running inside the Tauri runtime — not on the OS.
function canUseNativeUpdater() {
  return isTauriRuntime()
}

/**
 * Resolves the running app version from the Tauri runtime, falling back to the
 * static build version compiled into the bundle when the native API is
 * unavailable (web preview, tests).
 */
export async function getAppVersion(): Promise<string> {
  if (!canUseNativeUpdater()) {
    // Under Playwright, ignore the real bundled version so visual baselines
    // stay stable across version bumps; use the static mock build version.
    if (import.meta.env.VITE_PLAYWRIGHT) {
      return MOCK_APP_VERSION
    }
    return import.meta.env.VITE_APP_VERSION ?? '0.1.0'
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

/**
 * Downloads and applies an update, then relaunches the app. Accepts an already
 * fetched handle when the caller wants to install a specific fresh result, and
 * falls back to checking when called without one.
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
  // update. On macOS/Linux nothing restarts the process for us, so the freshly
  // installed binary only takes effect on the next launch unless we relaunch
  // explicitly here.
  if (isTauriRuntime() && !isWindowsRuntime()) {
    const { relaunch } = await import('@tauri-apps/plugin-process')
    await relaunch()
  }

  return true
}
