// The two-tone focus ring (see lib/focus-ring), drawn on the thumb when the
// transparent input above it has keyboard focus.
const THUMB_FOCUS_RING =
  'peer-focus-visible:ring-2 peer-focus-visible:ring-light-focus-outer peer-focus-visible:ring-offset-1 peer-focus-visible:ring-offset-light-focus-inner dark:peer-focus-visible:ring-dark-focus-outer dark:peer-focus-visible:ring-offset-dark-focus-inner'

/**
 * Fluent slider: a 4px track on the input boundary, accent fill, and a 20px
 * card thumb holding a 12px accent dot. A transparent native range input on
 * top keeps keyboard and pointer behaviour; the thumb shows its focus ring.
 */
export function RangeSlider({
  value,
  min,
  max,
  step = 1,
  onChange,
  ariaLabel,
  valueLabel,
}: {
  value: number
  min: number
  max: number
  step?: number
  onChange: (value: number) => void
  ariaLabel: string
  valueLabel?: string
}) {
  const percent = max > min ? ((value - min) / (max - min)) * 100 : 0
  return (
    <>
      <span className="relative flex h-5 flex-1 items-center">
        <input
          type="range"
          autoComplete="off"
          min={min}
          max={max}
          step={step}
          value={value}
          aria-label={ariaLabel}
          onChange={(event) => onChange(Number(event.target.value))}
          className="peer absolute inset-0 z-10 m-0 w-full cursor-pointer opacity-0"
        />
        <span className="h-1 w-full rounded-full bg-light-boundary opacity-55 dark:bg-dark-boundary" />
        {/* Runtime geometry: the fill and thumb follow the current value. */}
        <span
          className="absolute left-0 h-1 rounded-full bg-light-accent dark:bg-dark-accent"
          style={{ width: `${percent}%` }}
        />
        <span
          className={`absolute -ml-2.5 flex size-5 items-center justify-center rounded-full border border-light-control-stroke bg-light-card shadow-thumb dark:border-dark-control-stroke dark:bg-dark-card ${THUMB_FOCUS_RING}`}
          style={{ left: `${percent}%` }}
        >
          <span className="size-3 rounded-full bg-light-accent dark:bg-dark-accent" />
        </span>
      </span>
      <span className="w-14 flex-none text-right font-mono text-row text-light-text-secondary dark:text-dark-text-secondary">
        {valueLabel ?? value}
      </span>
    </>
  )
}
