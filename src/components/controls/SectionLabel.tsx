import type { ReactNode } from 'react'

export function SectionLabel({
  children,
  className = '',
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <h3
      className={`mb-1.5 mt-4.5 text-body font-semibold text-light-text first:mt-0 dark:text-dark-text ${className}`}
    >
      {children}
    </h3>
  )
}
