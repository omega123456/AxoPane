import type { ReactNode } from 'react'
import { XIcon } from '@/components/icons'
import { FOCUS_RING } from '@/lib/focus-ring'

export type InfoBarSeverity = 'info' | 'success' | 'warning' | 'error'

const SEVERITY_STYLES: Record<InfoBarSeverity, { fill: string; icon: string; glyph: string }> = {
  info: {
    fill: 'bg-light-info-bg dark:bg-dark-info-bg',
    icon: 'bg-light-accent text-light-on-accent dark:bg-dark-accent dark:text-dark-on-accent',
    glyph: 'i',
  },
  success: {
    fill: 'bg-light-success-bg dark:bg-dark-success-bg',
    icon: 'bg-light-success text-light-card dark:bg-dark-success dark:text-dark-card',
    glyph: '✓',
  },
  warning: {
    fill: 'bg-light-warning-bg dark:bg-dark-warning-bg',
    icon: 'bg-light-warning text-light-card dark:bg-dark-warning dark:text-dark-card',
    glyph: '!',
  },
  error: {
    fill: 'bg-light-danger-bg dark:bg-dark-danger-bg',
    icon: 'bg-light-danger text-light-card dark:bg-dark-danger dark:text-dark-card',
    glyph: '×',
  },
}

type InfoBarProps = {
  severity: InfoBarSeverity
  /** Bold lead-in shown inline before the message. */
  title?: string
  children?: ReactNode
  /** The one optional action, rendered as a standard button. */
  action?: ReactNode
  onDismiss?: () => void
  dismissLabel?: string
  dismissDisabled?: boolean
  /** Accessible name for the bar itself. */
  label?: string
  className?: string
}

/**
 * Windows InfoBar: a tinted fill per severity, a filled 16px status icon, the
 * bold title inline with the message, one action and a dismiss button.
 */
export function InfoBar({
  severity,
  title,
  children,
  action,
  onDismiss,
  dismissLabel = 'Dismiss',
  dismissDisabled = false,
  label,
  className = '',
}: InfoBarProps) {
  const style = SEVERITY_STYLES[severity]
  return (
    <div
      role={severity === 'error' ? 'alert' : 'status'}
      aria-label={label}
      data-severity={severity}
      className={`flex items-start gap-3 rounded-card border border-light-stroke py-3 pl-4 pr-2 text-body text-light-text dark:border-dark-stroke dark:text-dark-text ${style.fill} ${className}`}
    >
      <span
        aria-hidden="true"
        className={`mt-0.5 flex size-4 shrink-0 items-center justify-center rounded-full text-caption font-bold ${style.icon}`}
      >
        {style.glyph}
      </span>
      <p className="min-w-0 flex-1">
        {title ? <b className="mr-2 font-semibold">{title}</b> : null}
        {children}
      </p>
      {action ? <div className="-my-1.5 shrink-0">{action}</div> : null}
      {onDismiss ? (
        <button
          type="button"
          aria-label={dismissLabel}
          disabled={dismissDisabled}
          onClick={onDismiss}
          className={`-my-1 flex size-7 shrink-0 cursor-pointer items-center justify-center rounded-control text-light-text-secondary hover:bg-light-hover disabled:cursor-default disabled:opacity-40 dark:text-dark-text-secondary dark:hover:bg-dark-hover ${FOCUS_RING}`}
        >
          <XIcon className="size-4" />
        </button>
      ) : null}
    </div>
  )
}
