import type { ReactNode } from 'react'
import { Button } from '@/components/controls'
import { TEXT_SECONDARY } from '@/lib/class-names'

function stateIconToneClass(danger: boolean) {
  return danger ? 'text-light-danger dark:text-dark-danger' : TEXT_SECONDARY
}

export function StatePanel({
  icon,
  danger = false,
  title,
  message,
  action,
  role,
  label,
}: {
  icon: ReactNode
  danger?: boolean
  title: ReactNode
  message: ReactNode
  action?: { label: string; onClick: () => void }
  role: 'status' | 'alert'
  label?: string
}) {
  return (
    <div
      role={role}
      aria-label={label}
      className="flex min-h-0 flex-1 items-center justify-center overflow-auto px-6 py-10"
    >
      <div className="grid w-state justify-items-center gap-2.5 text-center">
        <span
          className={`flex size-state-tile items-center justify-center rounded-overlay bg-light-hover dark:bg-dark-hover ${stateIconToneClass(danger)}`}
        >
          {icon}
        </span>
        <h3 className="mt-1 break-words font-display text-subtitle font-semibold leading-6.5 text-light-text dark:text-dark-text">
          {title}
        </h3>
        <p className={`break-words text-row ${TEXT_SECONDARY}`}>{message}</p>
        {action ? <Button onClick={action.onClick}>{action.label}</Button> : null}
      </div>
    </div>
  )
}
