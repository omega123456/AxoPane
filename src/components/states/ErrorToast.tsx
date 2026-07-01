import { XCircleIcon, XIcon } from '@/components/icons'
import { useErrorToastStore } from '@/stores/error-toast-store'

export function ErrorToast() {
  const message = useErrorToastStore((state) => state.message)
  const dismiss = useErrorToastStore((state) => state.dismiss)

  if (!message) {
    return null
  }

  return (
    <div className="pointer-events-none absolute inset-x-0 top-2 z-30 flex justify-center px-3">
      <div
        role="alert"
        className="pointer-events-auto flex w-copycard items-center gap-2 rounded-window border border-light-border-strong bg-light-surface px-3 py-2.5 shadow-float dark:border-dark-border-strong dark:bg-dark-surface"
      >
        <XCircleIcon className="h-5 w-5 shrink-0 text-accent-red" />
        <span className="min-w-0 flex-1 truncate text-xs text-light-text dark:text-dark-text">
          {message}
        </span>
        <button
          type="button"
          aria-label="Dismiss error"
          onClick={dismiss}
          className="flex h-7 w-7 shrink-0 items-center justify-center rounded-md text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
        >
          <XIcon className="h-4 w-4" />
        </button>
      </div>
    </div>
  )
}
