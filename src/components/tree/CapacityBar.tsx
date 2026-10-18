import { capacityLevel } from '@/lib/volumes'

const FILL_CLASS_NAME = {
  normal: 'bg-light-accent dark:bg-dark-accent',
  warning: 'bg-light-warning dark:bg-dark-warning',
  critical: 'bg-light-danger dark:bg-dark-danger',
} as const

/** A drive's used space as a 3px bar under its name in the navigation pane. */
export function CapacityBar({ usedFraction }: { usedFraction: number }) {
  const level = capacityLevel(usedFraction)
  return (
    <span
      role="meter"
      aria-label="Used space"
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={Math.round(usedFraction * 100)}
      data-capacity-level={level}
      className="block h-0.75 w-capbar overflow-hidden rounded-full bg-light-divider dark:bg-dark-divider"
    >
      <span
        className={`block h-full rounded-full ${FILL_CLASS_NAME[level]}`}
        // Styling-constraint exception: runtime geometry only (the used share).
        style={{ width: `${usedFraction * 100}%` }}
      />
    </span>
  )
}
