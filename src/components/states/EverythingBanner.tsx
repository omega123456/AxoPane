import { AlertTriangleIcon, DownloadIcon, XIcon } from '@/components/icons'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'

function isWindows() {
  if (typeof navigator === 'undefined') {
    return false
  }

  return navigator.userAgent.includes('Windows')
}

export function EverythingBanner() {
  const everythingStatus = usePanesStore((state) => state.everythingStatus)
  const dismissed = useConfigStore((state) => state.dismissedEverythingBanner)
  const dismiss = useConfigStore((state) => state.dismissEverythingBanner)

  if (!isWindows() || dismissed || everythingStatus?.isAvailable) {
    return null
  }

  return (
    <div
      role="status"
      aria-label="Everything unavailable"
      className="flex h-crumb items-center gap-2 border-b border-light-border bg-accent-blue-soft px-3 text-row text-light-text-soft dark:border-dark-border dark:text-dark-text-soft"
    >
      <AlertTriangleIcon className="h-4 w-4 shrink-0 text-accent-amber" />
      <span className="min-w-0 flex-1 truncate">
        Folder sizes unavailable: Everything is not running.
      </span>
      <a
        href="https://www.voidtools.com/downloads/"
        target="_blank"
        rel="noreferrer"
        className="inline-flex h-7 shrink-0 items-center gap-1 rounded-tab px-2 text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
      >
        <DownloadIcon className="h-3.5 w-3.5" />
        Download
      </a>
      <button
        type="button"
        aria-label="Dismiss Everything banner"
        onClick={dismiss}
        className="inline-flex h-7 w-7 shrink-0 items-center justify-center rounded-tab text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
      >
        <XIcon className="h-3.5 w-3.5" />
      </button>
    </div>
  )
}
