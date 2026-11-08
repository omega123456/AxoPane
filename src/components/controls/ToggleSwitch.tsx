import { FOCUS_RING } from '@/lib/focus-ring'
import { TEXT_SECONDARY } from '@/lib/class-names'

const CHECKED_TRACK_CLASS =
  'border-light-accent bg-light-accent dark:border-dark-accent dark:bg-dark-accent'
const UNCHECKED_TRACK_CLASS =
  'border-light-boundary bg-light-control hover:bg-light-control-hover dark:border-dark-boundary dark:bg-dark-control dark:hover:bg-dark-control-hover'
const CHECKED_THUMB_CLASS = 'left-5.75 bg-light-on-accent dark:bg-dark-on-accent'
const UNCHECKED_THUMB_CLASS = 'left-0.75 bg-light-text-secondary dark:bg-dark-text-secondary'

function switchTrackClass(checked: boolean) {
  return checked ? CHECKED_TRACK_CLASS : UNCHECKED_TRACK_CLASS
}

function switchThumbClass(checked: boolean) {
  return checked ? CHECKED_THUMB_CLASS : UNCHECKED_THUMB_CLASS
}

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
    <span className={`inline-flex items-center gap-3 text-body ${TEXT_SECONDARY}`}>
      <span aria-hidden="true">{checked ? 'On' : 'Off'}</span>
      <button
        type="button"
        role="switch"
        aria-checked={checked}
        aria-label={label}
        onClick={() => onChange(!checked)}
        className={`relative h-5 w-10 shrink-0 cursor-pointer rounded-full border ${FOCUS_RING} ${switchTrackClass(checked)}`}
      >
        <span className={`absolute top-0.75 size-3 rounded-full ${switchThumbClass(checked)}`} />
      </button>
    </span>
  )
}
