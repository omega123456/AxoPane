import type { KeyboardEvent, ReactNode } from 'react'

export function DialogShell({
  children,
  label,
  onDismiss,
  onKeyDown,
}: {
  children: ReactNode
  label: string
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
        aria-label="Dismiss dialog"
        tabIndex={-1}
        onClick={onDismiss}
        className="absolute inset-0 cursor-default bg-dark-backdrop/40"
      />
      <div className="relative w-conflict overflow-hidden rounded-window border border-light-border-strong bg-light-surface shadow-window dark:border-dark-border-strong dark:bg-dark-surface">
        {children}
      </div>
    </div>
  )
}
