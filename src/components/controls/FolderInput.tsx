import type { KeyboardEvent, Ref } from 'react'

const EDITABLE_FOCUS_RING =
  'focus-within:ring-2 focus-within:ring-light-focus-outer dark:focus-within:ring-dark-focus-outer'

const NO_FOCUS_RING = ''

function wrapperFocusRing(readOnly: boolean) {
  return readOnly ? NO_FOCUS_RING : EDITABLE_FOCUS_RING
}

export function FolderInput({
  value,
  onChange,
  onKeyDown,
  ariaLabel,
  placeholder,
  readOnly = false,
  className = '',
  ref,
}: {
  value: string
  onChange?: (value: string) => void
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void
  ariaLabel: string
  placeholder?: string
  readOnly?: boolean
  className?: string
  ref?: Ref<HTMLInputElement>
}) {
  return (
    <div
      className={`flex h-9 items-center gap-2.5 rounded-control border border-light-control-stroke bg-light-layer px-3 dark:border-dark-control-stroke dark:bg-dark-layer ${wrapperFocusRing(readOnly)} ${className}`}
    >
      <span
        aria-hidden
        className="h-2.5 w-3 flex-none rounded-control border border-light-accent dark:border-dark-accent bg-light-tint dark:bg-dark-tint"
      />
      <input
        ref={ref}
        aria-label={ariaLabel}
        autoComplete="off"
        spellCheck={false}
        value={value}
        readOnly={readOnly}
        placeholder={placeholder}
        onChange={onChange ? (event) => onChange(event.target.value) : undefined}
        onKeyDown={onKeyDown}
        className="min-w-0 flex-1 select-text bg-transparent font-mono text-row text-light-text placeholder:text-light-text-tertiary focus-visible:outline-none dark:text-dark-text dark:placeholder:text-dark-text-tertiary"
      />
    </div>
  )
}
