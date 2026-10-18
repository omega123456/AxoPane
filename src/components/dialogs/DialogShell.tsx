import type { KeyboardEvent, ReactNode } from 'react'
import { AlertTriangleIcon } from '@/components/icons'

/**
 * Windows ContentDialog: a 420px card over a 40% scrim with a 20px semibold
 * title, a 14px body and a button band on the base colour. Footer buttons
 * share the band's width equally, primary first.
 */
export function DialogShell({
  children,
  label,
  title,
  footer,
  footerColumns = 2,
  dismissLabel = 'Dismiss dialog',
  onDismiss,
  onKeyDown,
}: {
  children?: ReactNode
  label: string
  title: ReactNode
  footer: ReactNode
  /** ContentDialog bands hold two buttons, or three with a secondary action. */
  footerColumns?: 2 | 3
  dismissLabel?: string
  onDismiss: () => void
  onKeyDown?: (event: KeyboardEvent) => void
}) {
  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-label={label}
      onKeyDown={onKeyDown}
      className="fixed inset-0 z-40 flex items-center justify-center"
    >
      <button
        type="button"
        aria-label={dismissLabel}
        tabIndex={-1}
        onClick={onDismiss}
        className="absolute inset-0 cursor-default bg-scrim"
      />
      <div className="relative w-dialog max-w-full overflow-hidden rounded-overlay border border-light-flyout-stroke bg-light-card text-light-text shadow-light-dialog dark:border-dark-flyout-stroke dark:bg-dark-card dark:text-dark-text dark:shadow-dark-dialog">
        <div className="grid gap-3 p-6">
          <h2 className="break-words font-display text-subtitle font-semibold">{title}</h2>
          {children}
        </div>
        <div
          className={`grid ${footerColumns === 3 ? 'grid-cols-3' : 'grid-cols-2'} gap-2 border-t border-light-divider bg-light-base px-6 py-5 dark:border-dark-divider dark:bg-dark-base`}
        >
          {footer}
        </div>
      </div>
    </div>
  )
}

export function DialogText({ children }: { children: ReactNode }) {
  return <p className="text-body">{children}</p>
}

/** Item list under the body text, in caption secondary. */
export function DialogList({ items }: { items: string[] }) {
  return (
    <ul className="grid gap-0.5 text-caption text-light-text-secondary dark:text-dark-text-secondary">
      {items.map((item, index) => (
        <li key={`${index}-${item}`} className="break-all">
          {item}
        </li>
      ))}
    </ul>
  )
}

export function DialogError({ message }: { message: string | null }) {
  return message ? (
    <p className="flex items-center gap-2 text-caption text-light-danger dark:text-dark-danger">
      <AlertTriangleIcon className="size-4 shrink-0" />
      {message}
    </p>
  ) : null
}

/** Caption label over a field, e.g. "Folder name". */
export function DialogField({
  label,
  htmlFor,
  children,
}: {
  label: string
  htmlFor: string
  children: ReactNode
}) {
  return (
    <div className="grid gap-1">
      <label
        htmlFor={htmlFor}
        className="text-caption text-light-text-secondary dark:text-dark-text-secondary"
      >
        {label}
      </label>
      {children}
    </div>
  )
}
