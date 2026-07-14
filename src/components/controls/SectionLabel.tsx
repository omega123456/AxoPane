import type { ReactNode } from 'react'

export function SectionLabel({
  children,
  className = '',
}: {
  children: ReactNode
  className?: string
}) {
  return (
    <div
      className={`text-2xs font-bold uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted ${className}`}
    >
      {children}
    </div>
  )
}
