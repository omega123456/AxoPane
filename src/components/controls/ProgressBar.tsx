export type ProgressTone = 'accent' | 'success' | 'danger' | 'neutral'

const toneFill: Record<ProgressTone, string> = {
  accent: 'bg-light-accent dark:bg-dark-accent',
  success: 'bg-light-success dark:bg-dark-success',
  danger: 'bg-light-danger dark:bg-dark-danger',
  neutral: 'bg-light-text-secondary dark:bg-dark-text-secondary',
}

/** Fluent determinate progress bar: a 4px divider track with an accent fill. */
export function ProgressBar({
  value,
  label,
  labelledBy,
  tone = 'accent',
  className = '',
}: {
  /** 0–100. */
  value: number
  label?: string
  labelledBy?: string
  tone?: ProgressTone
  className?: string
}) {
  const clamped = Math.min(100, Math.max(0, value))
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-labelledby={labelledBy}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(clamped)}
      className={`h-1 overflow-hidden rounded-full bg-light-divider dark:bg-dark-divider ${className}`}
    >
      {/* Runtime geometry: the fill width follows progress. */}
      <div className={`h-full rounded-full ${toneFill[tone]}`} style={{ width: `${clamped}%` }} />
    </div>
  )
}
