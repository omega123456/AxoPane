import { ArrowUpIcon, LockIcon } from '@/components/icons'

type PermissionDeniedProps = {
  onGoUp: () => void
  canGoUp: boolean
}

function isMac() {
  if (typeof navigator === 'undefined') {
    return false
  }

  return /Mac|iPhone|iPad/.test(navigator.userAgent)
}

export function PermissionDenied({ onGoUp, canGoUp }: PermissionDeniedProps) {
  const escapeLabel = isMac() ? 'Open in Terminal' : 'Open as Administrator'

  return (
    <div
      role="alert"
      aria-label="Permission denied"
      className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-10 text-center"
    >
      <LockIcon className="h-8 w-8 text-light-text-muted dark:text-dark-text-muted" />
      <p className="max-w-popover text-row text-light-text-soft dark:text-dark-text-soft">
        You do not have permission to view this folder.
      </p>
      <div className="flex items-center gap-2">
        <span className="inline-flex h-8 items-center rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-muted dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-muted">
          {escapeLabel}
        </span>
        <button
          type="button"
          onClick={onGoUp}
          disabled={!canGoUp}
          className="inline-flex h-8 items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:opacity-40 dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          <ArrowUpIcon className="h-4 w-4" />
          Go up
        </button>
      </div>
    </div>
  )
}
