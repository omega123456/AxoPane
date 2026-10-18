import { FOCUS_RING } from '@/lib/focus-ring'

/** Fluent toggle: 40×20 on the input boundary, with an On/Off label to its left. */
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
    <span className="inline-flex items-center gap-3 text-body text-light-text-secondary dark:text-dark-text-secondary">
      <span aria-hidden="true">{checked ? 'On' : 'Off'}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={`relative h-5 w-10 shrink-0 cursor-pointer rounded-full border ${FOCUS_RING} ${
          checked
            ? 'border-light-accent bg-light-accent dark:border-dark-accent dark:bg-dark-accent'
            : 'border-light-boundary bg-light-control hover:bg-light-control-hover dark:border-dark-boundary dark:bg-dark-control dark:hover:bg-dark-control-hover'
        }`}
      >
        <span
          className={`absolute top-0.75 size-3 rounded-full ${
            checked
              ? 'left-5.75 bg-light-on-accent dark:bg-dark-on-accent'
              : 'left-0.75 bg-light-text-secondary dark:bg-dark-text-secondary'
          }`}
        />
      </button>
    </span>
  )
}
