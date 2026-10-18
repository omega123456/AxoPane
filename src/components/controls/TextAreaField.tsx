import { FOCUS_RING } from '@/lib/focus-ring'
export function TextAreaField({
  label,
  description,
  value,
  onChange,
  ariaLabel,
  rows = 4,
}: {
  label?: string
  description?: string
  value: string
  onChange: (value: string) => void
  ariaLabel?: string
  rows?: number
}) {
  return (
    <div>
      {label ? (
        <div className="mb-1.5 text-row font-medium text-light-text dark:text-dark-text">
          {label}
        </div>
      ) : null}
      {description ? (
        <div className="mb-2 text-caption text-light-text-secondary dark:text-dark-text-secondary">
          {description}
        </div>
      ) : null}
      <textarea
        aria-label={ariaLabel ?? label}
        autoComplete="off"
        value={value}
        rows={rows}
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
        className={`w-full resize-y select-text rounded-control border border-light-control-stroke bg-light-layer p-3 font-mono text-caption leading-relaxed text-light-text-secondary dark:border-dark-control-stroke dark:bg-dark-layer dark:text-dark-text-secondary ${FOCUS_RING}`}
      />
    </div>
  )
}
