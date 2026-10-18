import type { InputHTMLAttributes, Ref } from 'react'

/**
 * Fluent text box: 32px, control fill, a boundary-coloured bottom stroke that
 * becomes a 2px accent underline on a card fill while focused. The underline is
 * the focus indicator, as in Windows.
 */
export function TextField({
  className = '',
  ref,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { ref?: Ref<HTMLInputElement> }) {
  return (
    <input
      ref={ref}
      autoComplete="off"
      spellCheck={false}
      className={`h-8 w-full min-w-0 select-text rounded-control border border-light-control-stroke border-b-light-boundary bg-light-control px-2.5 text-row text-light-text outline-none placeholder:text-light-text-tertiary focus:border-b-2 focus:border-b-light-accent focus:bg-light-card disabled:text-light-text-disabled dark:border-dark-control-stroke dark:border-b-dark-boundary dark:bg-dark-control dark:text-dark-text dark:placeholder:text-dark-text-tertiary dark:focus:border-b-dark-accent dark:focus:bg-dark-card dark:disabled:text-dark-text-disabled ${className}`}
      {...props}
    />
  )
}
