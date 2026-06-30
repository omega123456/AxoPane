import { ChevronDownIcon } from '@/components/icons'

export type SelectOption<T extends string> = { value: T; label: string }

export function SelectField<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
}: {
  value: T
  onChange: (value: T) => void
  options: SelectOption<T>[]
  ariaLabel: string
}) {
  return (
    <div className="relative inline-block">
      <select
        aria-label={ariaLabel}
        value={value}
        onChange={(event) => onChange(event.target.value as T)}
        className="h-9 w-50 cursor-pointer appearance-none rounded-tab border border-light-border-strong bg-light-surface pl-3 pr-9 text-row text-light-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border-strong dark:bg-dark-surface dark:text-dark-text"
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDownIcon className="pointer-events-none absolute right-3 top-1/2 size-4 -translate-y-1/2 text-light-text-muted dark:text-dark-text-muted" />
    </div>
  )
}
