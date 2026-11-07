import { FOCUS_RING_INSET } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'
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
  const cell = `flex size-8 items-center justify-center ${TEXT_SECONDARY} hover:bg-light-hover disabled:opacity-40 dark:hover:bg-dark-hover ${FOCUS_RING_INSET}`

  return (
    <div
      role="group"
      aria-label={ariaLabel}
      className="inline-flex flex-none items-center overflow-hidden rounded-control border border-light-control-stroke dark:border-dark-control-stroke"
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
      <div className="w-16 border-x border-light-control-stroke py-1.5 text-center font-mono text-row text-light-text dark:border-dark-control-stroke dark:text-dark-text">
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
