import { AlertTriangleIcon, ArrowUpIcon, RefreshIcon } from '@/components/icons'

type ErrorStateProps = {
  message: string
  onRetry: () => void
  onGoUp: () => void
  canGoUp: boolean
}

export function ErrorState({ message, onRetry, onGoUp, canGoUp }: ErrorStateProps) {
  return (
    <div
      role="alert"
      className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-10 text-center"
    >
      <AlertTriangleIcon className="h-8 w-8 text-accent-amber" />
      <p className="max-w-popover text-row text-light-text-soft dark:text-dark-text-soft">
        {message}
      </p>
      <div className="flex items-center gap-2">
        <button
          type="button"
          onClick={onRetry}
          className="inline-flex h-8 items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
        >
          <RefreshIcon className="h-4 w-4" />
          Try again
        </button>
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
