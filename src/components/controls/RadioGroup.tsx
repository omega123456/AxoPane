import { FOCUS_RING } from '@/lib/focus-ring'
export type RadioOption<T extends string> = { value: T; label: string }

const SELECTED_RING_CLASS =
  'border-5 border-light-accent bg-light-on-accent dark:border-dark-accent dark:bg-dark-on-accent'
const UNSELECTED_RING_CLASS =
  'border border-light-boundary bg-light-control dark:border-dark-boundary dark:bg-dark-control'

function radioRingClass(selected: boolean) {
  return selected ? SELECTED_RING_CLASS : UNSELECTED_RING_CLASS
}

export function RadioGroup<T extends string>({
  value,
  onChange,
  options,
  ariaLabel,
}: {
  value: T
  onChange: (value: T) => void
  options: RadioOption<T>[]
  ariaLabel: string
}) {
  return (
    <div role="radiogroup" aria-label={ariaLabel} className="grid gap-1">
      {options.map((option) => {
        const selected = option.value === value
        return (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={selected}
            onClick={() => onChange(option.value)}
            className={`flex items-center gap-2 rounded-control py-1.5 text-left ${FOCUS_RING}`}
          >
            <span className={`size-5 flex-none rounded-full ${radioRingClass(selected)}`} />
            <span className="text-row text-light-text dark:text-dark-text">{option.label}</span>
          </button>
        )
      })}
    </div>
  )
}
