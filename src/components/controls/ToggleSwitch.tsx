export function ToggleSwitch({
  checked,
  onChange,
  label,
}: {
  checked: boolean
  onChange: (value: boolean) => void
  label?: string
}) {
  return (
    <button
      type="button"
      role="switch"
      aria-checked={checked}
      aria-label={label}
      onClick={() => onChange(!checked)}
      className={`relative h-switch-h w-switch-w shrink-0 cursor-pointer rounded-full transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
        checked
          ? 'bg-accent-blue-light dark:bg-accent-blue'
          : 'bg-light-border-strong dark:bg-dark-border-strong'
      }`}
    >
      <span
        className={`absolute top-switch-pad size-4 rounded-full transition-all ${
          checked
            ? 'left-switch-shift bg-white shadow-sm'
            : 'left-switch-pad bg-light-text-muted dark:bg-dark-text-muted'
        }`}
      />
    </button>
  )
}
