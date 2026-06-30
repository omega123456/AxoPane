import type { ButtonHTMLAttributes, ReactNode } from 'react'

type Variant = 'default' | 'primary' | 'ghost'

const variants: Record<Variant, string> = {
  default:
    'border border-light-border-strong px-4.5 py-2 text-light-text-soft hover:bg-light-hover hover:text-light-text dark:border-dark-border-strong dark:text-dark-text-soft dark:hover:bg-dark-hover dark:hover:text-dark-text',
  primary:
    'bg-accent-blue-strong px-5.5 py-2 font-semibold text-accent-blue-light hover:bg-accent-blue-soft dark:text-accent-blue',
  ghost:
    'px-2.5 py-2 text-light-text-muted hover:bg-light-hover hover:text-light-text-soft dark:text-dark-text-muted dark:hover:bg-dark-hover dark:hover:text-dark-text-soft',
}

export function Button({
  variant = 'default',
  children,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant; children: ReactNode }) {
  return (
    <button
      type="button"
      className={`cursor-pointer rounded-tab text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border disabled:cursor-default ${variants[variant]} ${className}`}
      {...props}
    >
      {children}
    </button>
  )
}
