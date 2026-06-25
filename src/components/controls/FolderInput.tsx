export function FolderInput({
  value,
  onChange,
  ariaLabel,
  readOnly = false,
}: {
  value: string
  onChange?: (value: string) => void
  ariaLabel: string
  readOnly?: boolean
}) {
  return (
    <div className="flex h-9 items-center gap-2.5 rounded-tab border border-light-border-strong bg-light-surface px-3 dark:border-dark-border-strong dark:bg-dark-surface">
      <span
        aria-hidden
        className="h-2.5 w-3 flex-none rounded-sm border border-accent-blue-border bg-accent-blue-soft"
      />
      <input
        aria-label={ariaLabel}
        value={value}
        readOnly={readOnly}
        onChange={onChange ? (event) => onChange(event.target.value) : undefined}
        className="min-w-0 flex-1 bg-transparent font-mono text-row text-light-text focus-visible:outline-none dark:text-dark-text"
      />
    </div>
  )
}
