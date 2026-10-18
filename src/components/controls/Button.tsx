import type { ButtonHTMLAttributes, ReactNode, Ref } from 'react'
import { FOCUS_RING } from '@/lib/focus-ring'

type Variant = 'default' | 'primary' | 'subtle'
type Size = 'default' | 'icon' | 'compact'

// The disabled look of the filled buttons: a pressed fill with disabled text.
const FILLED_DISABLED =
  'disabled:cursor-default disabled:border-light-control-stroke disabled:bg-light-press disabled:text-light-text-disabled dark:disabled:border-dark-control-stroke dark:disabled:bg-dark-press dark:disabled:text-dark-text-disabled'

// Fluent buttons: 32px tall, 4px corners. The standard button's bottom stroke
// is a shade darker in light mode, which gives it its subtle depth.
const variants: Record<Variant, string> = {
  default: `border-light-control-stroke border-b-light-control-stroke-bottom bg-light-control text-light-text hover:bg-light-control-hover active:bg-light-control-press dark:border-dark-control-stroke dark:border-b-dark-control-stroke-bottom dark:bg-dark-control dark:text-dark-text dark:hover:bg-dark-control-hover dark:active:bg-dark-control-press ${FILLED_DISABLED}`,
  primary: `border-transparent bg-light-accent text-light-on-accent hover:bg-light-accent/90 active:bg-light-accent/80 dark:bg-dark-accent dark:text-dark-on-accent dark:hover:bg-dark-accent/90 dark:active:bg-dark-accent/80 ${FILLED_DISABLED}`,
  subtle:
    'border-transparent text-light-text hover:bg-light-hover active:bg-light-press disabled:cursor-default disabled:bg-transparent disabled:text-light-text-disabled dark:text-dark-text dark:hover:bg-dark-hover dark:active:bg-dark-press dark:disabled:text-dark-text-disabled',
}

// Icon buttons are square: 32px, or 24px for the compact ones inside a dense row.
const sizes: Record<Size, string> = {
  default: 'h-8 px-3',
  icon: 'size-8',
  compact: 'size-6',
}

export function Button({
  variant = 'default',
  size = 'default',
  children,
  className = '',
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: Variant
  size?: Size
  children: ReactNode
  ref?: Ref<HTMLButtonElement>
}) {
  return (
    <button
      type="button"
      className={`inline-flex shrink-0 cursor-pointer items-center justify-center gap-2 whitespace-nowrap rounded-control border text-row ${sizes[size]} ${variants[variant]} ${FOCUS_RING} ${className}`}
      {...props}
    >
      {children}
    </button>
  )
}
