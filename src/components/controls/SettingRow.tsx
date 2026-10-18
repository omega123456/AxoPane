import type { ReactNode } from 'react'

/** Windows Settings card: icon, title and one-line description, control on the right. */
export function SettingRow({
  icon,
  title,
  description,
  control,
}: {
  icon?: ReactNode
  title: ReactNode
  description?: ReactNode
  control: ReactNode
}) {
  return (
    <div className="flex min-h-16 items-center gap-4 rounded-card border border-light-stroke bg-light-card px-4 py-2.5 dark:border-dark-stroke dark:bg-dark-card">
      {icon ? (
        <span className="flex size-4 shrink-0 text-light-text dark:text-dark-text">{icon}</span>
      ) : null}
      <div className="grid min-w-0 flex-1">
        <div className="text-body text-light-text dark:text-dark-text">{title}</div>
        {description ? (
          <div className="text-caption text-light-text-secondary dark:text-dark-text-secondary">
            {description}
          </div>
        ) : null}
      </div>
      <div className="flex shrink-0 items-center gap-3">{control}</div>
    </div>
  )
}
