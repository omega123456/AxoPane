import { FOCUS_RING_INSET } from '@/lib/focus-ring'
export type SegmentedOption<T extends string> = { value: T; label: string }

/** Fluent segmented control: 26px items in a 2px-padded control well; selection is solid accent. */
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
      className="inline-flex gap-0.5 rounded-card border border-light-control-stroke bg-light-control p-0.5 dark:border-dark-control-stroke dark:bg-dark-control"
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
            className={`flex h-6.5 cursor-pointer items-center rounded-control px-3 text-row ${FOCUS_RING_INSET} ${
              active
                ? 'bg-light-accent text-light-on-accent dark:bg-dark-accent dark:text-dark-on-accent'
                : 'text-light-text hover:bg-light-hover dark:text-dark-text dark:hover:bg-dark-hover'
            }`}
          >
            {option.label}
          </button>
        )
      })}
    </div>
  )
}
