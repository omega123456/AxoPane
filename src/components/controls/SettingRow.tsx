import type { ReactNode } from 'react'

export function SettingRow({
  title,
  description,
  control,
  fixedCopy = false,
}: {
  title: ReactNode
  description?: ReactNode
  control: ReactNode
  fixedCopy?: boolean
}) {
  return (
    <div className="flex items-center gap-4 border-b border-light-border py-3 dark:border-dark-border">
      <div className={fixedCopy ? 'w-50 flex-none' : 'min-w-0 flex-1'}>
        <div className="text-row font-medium text-light-text dark:text-dark-text">{title}</div>
        {description ? (
          <div className="mt-0.5 text-uxs text-light-text-muted dark:text-dark-text-muted">{description}</div>
        ) : null}
      </div>
      {control}
    </div>
  )
}
