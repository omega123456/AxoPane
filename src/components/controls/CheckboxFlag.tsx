import type { ReactNode } from 'react'
import { CheckIcon } from '@/components/icons'
import { FOCUS_RING } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'

const CHECKED_BOX_CLASS =
  'border-light-accent bg-light-accent text-light-on-accent dark:border-dark-accent dark:bg-dark-accent dark:text-dark-on-accent'
const UNCHECKED_BOX_CLASS =
  'border-light-boundary bg-light-control dark:border-dark-boundary dark:bg-dark-control'

function checkboxBoxClass(checked: boolean) {
  return checked ? CHECKED_BOX_CLASS : UNCHECKED_BOX_CLASS
}

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
        className={`flex size-4.5 flex-none items-center justify-center rounded-control border ${checkboxBoxClass(checked)}`}
      >
        {checked ? <CheckIcon className="size-3" strokeWidth={2.25} /> : null}
      </span>
      <span className="min-w-0 flex-1">
        <span className="block text-row text-light-text dark:text-dark-text">{title}</span>
        {description ? (
          <span className={`block text-caption ${TEXT_SECONDARY}`}>{description}</span>
        ) : null}
      </span>
    </button>
  )
}
