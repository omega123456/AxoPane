export function Stepper({
  value,
  onChange,
  min,
  max,
  step = 1,
  format,
  ariaLabel,
}: {
  value: number
  onChange: (value: number) => void
  min: number
  max: number
  step?: number
  format?: (value: number) => string
  ariaLabel: string
}) {
  const cell =
    'flex size-8 items-center justify-center text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border disabled:opacity-40 dark:text-dark-text-soft dark:hover:bg-dark-hover'

  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className="inline-flex flex-none items-center overflow-hidden rounded-tab border border-light-border-strong dark:border-dark-border-strong"
    >
      <button
        type="button"
        aria-label="Decrease"
        disabled={value <= min}
        onClick={() => onChange(Math.max(min, value - step))}
        className={cell}
      >
        −
      </button>
      <div className="w-16 border-x border-light-border-strong py-1.5 text-center font-mono text-row text-light-text dark:border-dark-border-strong dark:text-dark-text">
        {format ? format(value) : value}
      </div>
      <button
        type="button"
        aria-label="Increase"
        disabled={value >= max}
        onClick={() => onChange(Math.min(max, value + step))}
        className={cell}
      >
        +
      </button>
    </div>
  )
}
