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
    <div className="flex h-9 items-center gap-2.5 rounded-control border border-light-control-stroke bg-light-layer px-3 dark:border-dark-control-stroke dark:bg-dark-layer">
      <span
        aria-hidden
        className="h-2.5 w-3 flex-none rounded-control border border-light-accent dark:border-dark-accent bg-light-tint dark:bg-dark-tint"
      />
      <input
        aria-label={ariaLabel}
        autoComplete="off"
        value={value}
        readOnly={readOnly}
        onChange={onChange ? (event) => onChange(event.target.value) : undefined}
        className="min-w-0 flex-1 select-text bg-transparent font-mono text-row text-light-text focus-visible:outline-none dark:text-dark-text"
      />
    </div>
  )
}
