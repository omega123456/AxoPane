import { ChevronDownIcon } from '@/components/icons'
import { FOCUS_RING } from '@/lib/focus-ring'

export type SelectOption<T extends string> = { value: T; label: string }

/** Fluent combo box: 32px, control fill and strokes, 12px chevron. */
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
        className={`h-8 min-w-35 cursor-pointer appearance-none rounded-control border border-light-control-stroke border-b-light-control-stroke-bottom bg-light-control pl-3 pr-8.5 text-row text-light-text hover:bg-light-control-hover dark:border-dark-control-stroke dark:border-b-dark-control-stroke-bottom dark:bg-dark-control dark:text-dark-text dark:hover:bg-dark-control-hover ${FOCUS_RING}`}
      >
        {options.map((option) => (
          <option key={option.value} value={option.value}>
            {option.label}
          </option>
        ))}
      </select>
      <ChevronDownIcon className="pointer-events-none absolute right-2.5 top-1/2 size-3 -translate-y-1/2 text-light-text-secondary dark:text-dark-text-secondary" />
    </div>
  )
}
