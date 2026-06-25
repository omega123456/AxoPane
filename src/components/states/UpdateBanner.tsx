import { AlertTriangleIcon, DownloadIcon, XIcon } from '@/components/icons'
import { log } from '@/lib/app-log-commands'
import { downloadAndInstallAppUpdate } from '@/lib/updater'
import { useUpdaterStore } from '@/stores/updater-store'

export function UpdateBanner() {
  const summary = useUpdaterStore((state) => state.summary)
  const update = useUpdaterStore((state) => state.update)
  const status = useUpdaterStore((state) => state.status)
  const error = useUpdaterStore((state) => state.error)
  const setStatus = useUpdaterStore((state) => state.setStatus)
  const dismiss = useUpdaterStore((state) => state.dismiss)

  if (!summary || !update) {
    return null
  }

  const installing = status === 'installing'

  async function install() {
    if (!update || installing) {
      return
    }

    setStatus('installing')
    try {
      await downloadAndInstallAppUpdate(update)
    } catch (cause) {
      const message = cause instanceof Error ? cause.message : String(cause)
      log.error('app update install failed', { error: message })
      setStatus('error', message)
    }
  }

  return (
    <div
      role="status"
      aria-label="Update available"
      className="flex h-crumb items-center gap-2 border-b border-light-border bg-accent-blue-soft px-3 text-row text-light-text-soft dark:border-dark-border dark:text-dark-text-soft"
    >
      <DownloadIcon className="h-4 w-4 shrink-0 text-accent-blue-light dark:text-accent-blue" />
      <span className="min-w-0 flex-1 truncate">
        {status === 'error' ? (
          <span className="inline-flex items-center gap-1 text-accent-amber">
            <AlertTriangleIcon className="h-3.5 w-3.5 shrink-0" />
            Update failed: {error}
          </span>
        ) : (
          `Update available: ${summary.version} (current ${summary.currentVersion}).`
        )}
      </span>
      <button
        type="button"
        disabled={installing}
        onClick={() => void install()}
        className="inline-flex h-7 shrink-0 items-center gap-1 rounded-tab px-2 text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-accent-blue"
      >
        {installing ? 'Installing…' : 'Install & restart'}
      </button>
      <button
        type="button"
        aria-label="Dismiss update banner"
        disabled={installing}
        onClick={dismiss}
        className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-tab text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
      >
        <XIcon className="h-3.5 w-3.5" />
      </button>
    </div>
  )
}
