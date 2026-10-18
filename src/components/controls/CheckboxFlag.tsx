import type { ReactNode } from 'react'
import { CheckIcon } from '@/components/icons'
import { FOCUS_RING } from '@/lib/focus-ring'

/** Fluent checkbox: an 18px box on the input boundary, solid accent when checked. */
export function CheckboxFlag({
  checked,
  onChange,
  title,
  description,
  tooltip,
  keyShortcut,
}: {
  checked: boolean
  onChange: (value: boolean) => void
  title: ReactNode
  description?: ReactNode
  tooltip?: string
  keyShortcut?: string
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      aria-keyshortcuts={keyShortcut}
      title={tooltip}
      onClick={() => onChange(!checked)}
      className={`flex items-start gap-2 rounded-control text-left ${FOCUS_RING}`}
    >
      <span
        className={`flex size-4.5 flex-none items-center justify-center rounded-control border ${
          checked
            ? 'border-light-accent bg-light-accent text-light-on-accent dark:border-dark-accent dark:bg-dark-accent dark:text-dark-on-accent'
            : 'border-light-boundary bg-light-control dark:border-dark-boundary dark:bg-dark-control'
        }`}
      >
        {checked ? <CheckIcon className="size-3" strokeWidth={2.25} /> : null}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-row text-light-text dark:text-dark-text">{title}</span>
        {description ? (
          <span className="block text-caption text-light-text-secondary dark:text-dark-text-secondary">
            {description}
          </span>
        ) : null}
      </span>
    </button>
  )
}
