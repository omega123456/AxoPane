/** Human-readable byte size, e.g. `18.4 GB`. */
export function formatBytes(value: number): string {
  if (value <= 0) {
    return '0 B'
  }
  const units = ['B', 'KB', 'MB', 'GB', 'TB']
  let current = value
  let unitIndex = 0
  while (current >= 1024 && unitIndex < units.length - 1) {
    current /= 1024
    unitIndex += 1
  }
  return `${current.toFixed(unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

/** Transfer rate per second, e.g. `248 MB/s`. */
export function formatRate(bytesPerSecond: number): string {
  return `${formatBytes(bytesPerSecond)}/s`
}

/** Coarse "about N min left" / "about N sec left" ETA phrasing. */
export function formatEta(seconds: number): string {
  if (seconds < 60) {
    return `about ${Math.max(1, Math.round(seconds))} sec left`
  }
  const minutes = Math.round(seconds / 60)
  if (minutes < 60) {
    return `about ${minutes} min left`
  }
  const hours = Math.round(minutes / 60)
  return `about ${hours} hr left`
}

/** Integer with thousands separators, e.g. `1,248`. */
export function formatCount(value: number): string {
  return value.toLocaleString('en-US')
}
