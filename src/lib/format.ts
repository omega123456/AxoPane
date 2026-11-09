import { minutesInHour, secondsInMinute } from 'date-fns/constants'
import { BYTES_PER_KIBIBYTE } from '@/lib/numbers'

export function formatBytes(value: number): string {
  if (value <= 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let current = value
  let unitIndex = 0
  const largestUnitIndex = units.length - 1
  const canScaleUp = () => current >= BYTES_PER_KIBIBYTE && unitIndex < largestUnitIndex
  while (canScaleUp()) {
    current /= BYTES_PER_KIBIBYTE
    unitIndex += 1
  }
  return `${current.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

export function formatDuration(seconds: number): string {
  if (seconds < secondsInMinute) {
    return `${Math.max(1, Math.round(seconds))} sec`
  }
  const minutes = Math.round(seconds / secondsInMinute)
  if (minutes < minutesInHour) {
    return `${minutes} min`
  }
  return `${Math.round(minutes / minutesInHour)} hr`
}

export function formatEta(seconds: number): string {
  return `about ${formatDuration(seconds)} left`
}

export const DETAIL_SEPARATOR = ' · '

export const EMPTY_VALUE = '—'

export function formatCount(value: number): string {
  return value.toLocaleString('en-US')
}

export function itemNoun(count: number) {
  return count === 1 ? 'item' : 'items'
}

export function formatItemCount(count: number) {
  return `${formatCount(count)} ${itemNoun(count)}`
}
