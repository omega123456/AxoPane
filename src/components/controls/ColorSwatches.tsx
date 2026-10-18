import { FOCUS_RING } from '@/lib/focus-ring'
export function ColorSwatches({
  value,
  onChange,
  swatches,
}: {
  value: string
  onChange: (value: string) => void
  swatches: string[]
}) {
  return (
    <div className="flex flex-wrap items-center gap-2.5">
      {swatches.map((color) => {
        const selected = color.toLowerCase() === value.toLowerCase()
        return (
          <button
            key={color}
            type="button"
            aria-label={`Accent ${color}`}
            aria-pressed={selected}
            onClick={() => onChange(color)}
            className={`flex size-swatch items-center justify-center rounded-control ${FOCUS_RING}`}
            // Styling exception: a swatch's fill is user palette data, not a
            // design-system token, so it can only be expressed at runtime.
            style={{ backgroundColor: color }}
          >
            {selected ? (
              <span className="size-3 rounded-full bg-white ring-2 ring-light-accent dark:ring-dark-accent" />
            ) : null}
          </button>
        )
      })}
      <span className="h-6 w-px bg-light-control-stroke dark:bg-dark-control-stroke" />
      <label className="inline-flex cursor-pointer items-center gap-2.5">
        <input
          type="color"
          autoComplete="off"
          value={value}
          aria-label="Custom accent color"
          onChange={(event) => onChange(event.target.value)}
          className="size-color-input cursor-pointer rounded-control border border-light-control-stroke bg-light-layer p-0.5 dark:border-dark-control-stroke dark:bg-dark-layer"
        />
        <span className="font-mono text-row uppercase text-light-text-secondary dark:text-dark-text-secondary">
          {value}
        </span>
      </label>
    </div>
  )
}
