export type RadioOption<T extends string> = { value: T; label: string }

export function RadioGroup<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
}: {
  value: T
  onChange: (value: T) => void
  options: RadioOption<T>[]
  ariaLabel: string
}) {
  return (
    <div role="radiogroup" aria-label={ariaLabel}>
      {options.map((option) => {
        const selected = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(option.value)}
            className="flex w-full items-center gap-2.5 rounded-tab py-2 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
          >
            <span
              className={`flex size-control-dot flex-none items-center justify-center rounded-full border ${
                selected
                  ? 'border-accent-blue-light dark:border-accent-blue'
                  : 'border-light-border-strong dark:border-dark-border-strong'
              }`}
            >
              {selected ? (
                <span className="size-radio-dot rounded-full bg-accent-blue-light dark:bg-accent-blue" />
              ) : null}
            </span>
            <span className="text-row text-light-text dark:text-dark-text">{option.label}</span>
          </button>
        )
      })}
    </div>
  )
}
