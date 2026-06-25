export type SegmentedOption<T extends string> = { value: T; label: string }

export function SegmentedControl<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
}: {
  value: T
  onChange: (value: T) => void
  options: SegmentedOption<T>[]
  ariaLabel: string
}) {
  return (
    <div
      role="radiogroup"
      aria-label={ariaLabel}
      className="inline-flex gap-0.5 rounded-lg border border-light-border bg-light-surface p-0.5 dark:border-dark-border dark:bg-dark-surface"
    >
      {options.map((option) => {
        const active = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={active}
            onClick={() => onChange(option.value)}
            className={`rounded-md px-4 py-1.5 text-usm capitalize focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
              active
                ? 'bg-accent-blue-strong font-semibold text-accent-blue-light dark:text-accent-blue'
                : 'text-light-text-muted dark:text-dark-text-muted'
            }`}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
