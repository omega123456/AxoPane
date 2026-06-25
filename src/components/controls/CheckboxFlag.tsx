import type { ReactNode } from 'react'
import { CheckIcon } from '@/components/icons'

export function CheckboxFlag({
  checked,
  onChange,
  title,
  description,
}: {
  checked: boolean
  onChange: (value: boolean) => void
  title: ReactNode
  description?: ReactNode
}) {
  return (
    <button
      type="button"
      role="checkbox"
      aria-checked={checked}
      onClick={() => onChange(!checked)}
      className="flex w-full items-start gap-2.5 rounded-tab border-b border-light-border py-2.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border"
    >
      <span
        className={`mt-0.5 flex size-control-dot flex-none items-center justify-center rounded-md ${
          checked
            ? 'bg-accent-blue-light text-white dark:bg-accent-blue'
            : 'border border-light-border-strong dark:border-dark-border-strong'
        }`}
      >
        {checked ? <CheckIcon className="size-3" /> : null}
      </span>
      <div className="min-w-0 flex-1">
        <div className="text-row font-medium text-light-text dark:text-dark-text">{title}</div>
        {description ? (
          <div className="mt-0.5 text-uxs text-light-text-muted dark:text-dark-text-muted">{description}</div>
        ) : null}
      </div>
    </button>
  )
}
