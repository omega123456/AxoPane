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
      {label ? <div className="mb-1.5 text-row font-medium text-light-text dark:text-dark-text">{label}</div> : null}
      {description ? (
        <div className="mb-2 text-uxs text-light-text-muted dark:text-dark-text-muted">{description}</div>
      ) : null}
      <textarea
        aria-label={ariaLabel ?? label}
        value={value}
        rows={rows}
        spellCheck={false}
        onChange={(event) => onChange(event.target.value)}
        className="w-full resize-y select-text rounded-tab border border-light-border-strong bg-light-surface p-3 font-mono text-uxs leading-relaxed text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border-strong dark:bg-dark-surface dark:text-dark-text-soft"
      />
    </div>
  )
}
