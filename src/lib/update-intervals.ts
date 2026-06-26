export const UPDATE_INTERVAL_OPTIONS = [
  { value: '1h', label: 'Every hour', ms: 3_600_000 },
  { value: '5h', label: 'Every 5 hours', ms: 18_000_000 },
  { value: '1d', label: 'Every day', ms: 86_400_000 },
  { value: '7d', label: 'Every 7 days', ms: 604_800_000 },
  { value: 'off', label: 'Off', ms: null },
] as const

export type UpdateInterval = (typeof UPDATE_INTERVAL_OPTIONS)[number]['value']

export const DEFAULT_UPDATE_INTERVAL: UpdateInterval = '1d'

export const UPDATE_INTERVAL_MS = Object.fromEntries(
  UPDATE_INTERVAL_OPTIONS.filter((option) => option.ms !== null).map((option) => [
    option.value,
    option.ms,
  ]),
) as Record<Exclude<UpdateInterval, 'off'>, number>

export function isUpdateInterval(value: string): value is UpdateInterval {
  return UPDATE_INTERVAL_OPTIONS.some((option) => option.value === value)
}
