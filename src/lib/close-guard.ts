import { hasUnfinishedOps } from '@/lib/queue-commands'

type CloseEvent = { preventDefault: () => void }

export type GuardableWindow = {
  onCloseRequested: (handler: (event: CloseEvent) => void | Promise<void>) => Promise<() => void>
  destroy: () => Promise<void>
}

/** Resolve the real Tauri window, or `null` outside the desktop app. */
async function defaultAcquireWindow(): Promise<GuardableWindow | null> {
  if (!globalThis.__TAURI_IPC__) {
    return null
  }
  try {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()
    return {
      onCloseRequested: (handler) => appWindow.onCloseRequested((event) => handler(event)),
      destroy: () => appWindow.destroy(),
    }
  } catch {
    // No real Tauri runtime (web build / test env): nothing to guard.
    return null
  }
}

function defaultConfirm(message: string): boolean {
  return typeof window !== 'undefined' && typeof window.confirm === 'function'
    ? window.confirm(message)
    : true
}

/**
 * Install a guard that prevents the window from closing while queue jobs are
 * active or pending, requiring an explicit confirmation. Returns an unlisten
 * function. `acquireWindow` and `confirmClose` are injectable for testing.
 */
export async function installCloseGuard(
  acquireWindow: () => Promise<GuardableWindow | null> = defaultAcquireWindow,
  confirmClose: (message: string) => boolean = defaultConfirm,
): Promise<() => void> {
  const appWindow = await acquireWindow()
  if (!appWindow) {
    return () => {}
  }

  return appWindow.onCloseRequested(async (event) => {
    const unfinished = await hasUnfinishedOps()
    if (!unfinished) {
      return
    }
    const proceed = confirmClose(
      'Jobs are still in progress. Closing now will stop them. Close anyway?',
    )
    if (!proceed) {
      event.preventDefault()
      return
    }
    await appWindow.destroy()
  })
}
