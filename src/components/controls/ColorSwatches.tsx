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
            className="flex size-swatch items-center justify-center rounded-tab focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            // Styling exception: a swatch's fill is user palette data, not a
            // design-system token, so it can only be expressed at runtime.
            style={{ backgroundColor: color }}
          >
            {selected ? <span className="size-3 rounded-full bg-white ring-2 ring-accent-blue" /> : null}
          </button>
        )
      })}
      <span className="h-6 w-px bg-light-border-strong dark:bg-dark-border-strong" />
      <label className="inline-flex cursor-pointer items-center gap-2.5">
        <input
          type="color"
          value={value}
          aria-label="Custom accent color"
          onChange={(event) => onChange(event.target.value)}
          className="size-color-input cursor-pointer rounded-tab border border-light-border-strong bg-light-surface p-0.5 dark:border-dark-border-strong dark:bg-dark-surface"
        />
        <span className="font-mono text-row uppercase text-light-text-soft dark:text-dark-text-soft">{value}</span>
      </label>
    </div>
  )
}
