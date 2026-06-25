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
  return (
    <>
      <input
        type="range"
        min={min}
        max={max}
        step={step}
        value={value}
        aria-label={ariaLabel}
        onChange={(event) => onChange(Number(event.target.value))}
        className="h-1 flex-1 cursor-pointer rounded-full accent-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:accent-accent-blue"
      />
      <span className="w-14 flex-none text-right font-mono text-row text-light-text-soft dark:text-dark-text-soft">
        {valueLabel ?? value}
      </span>
    </>
  )
}
