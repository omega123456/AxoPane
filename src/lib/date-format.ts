import { format } from 'date-fns'

/**
 * Date formatting for directory listings and the properties dialog.
 *
 * Backend timestamps arrive as RFC3339 strings. Absolute values are rendered in
 * the user's local timezone via `date-fns` (the `format` helper localises the
 * instant and supplies ordinals through the `do` token). The time portion is
 * appended only when `showTime` is set — with seconds only when `showSeconds`
 * is also set — so it composes with any date format rather than multiplying the
 * catalogue. The optional relative mode is purely diff-based (timezone
 * independent): it replaces the absolute value with phrases like `15 minutes
 * ago` up to a 2-day cutoff, after which it falls back to the chosen format.
 */

export const DATE_FORMATS = ['ymd', 'dmy', 'mdy', 'med', 'dme', 'lmd', 'dml', 'dly'] as const

export type DateFormat = (typeof DATE_FORMATS)[number]

export const DEFAULT_DATE_FORMAT: DateFormat = 'ymd'

export function isDateFormat(value: string): value is DateFormat {
  return (DATE_FORMATS as readonly string[]).includes(value)
}

/** `date-fns` pattern per format (date portion only). */
const datePatterns: Record<DateFormat, string> = {
  ymd: 'yyyy-MM-dd',
  dmy: 'dd/MM/yyyy',
  mdy: 'MM/dd/yyyy',
  med: 'MMM do yyyy',
  dme: 'do MMM yyyy',
  lmd: 'MMMM d, yyyy',
  dml: 'd MMMM yyyy',
  dly: 'do MMMM yy',
}

/** Example-driven labels shown in the settings dropdown (date portion only). */
export const dateFormatLabels: Record<DateFormat, string> = {
  ymd: '2026-06-30',
  dmy: '30/06/2026',
  mdy: '06/30/2026',
  med: 'Jun 30th 2026',
  dme: '30th Jun 2026',
  lmd: 'June 30, 2026',
  dml: '30 June 2026',
  dly: '30th June 26',
}

/**
 * Recency tone for the dynamic/relative display. `default` is the un-coloured
 * tone used for absolute values (relative mode off, or beyond the cutoff).
 */
export type DateTone = 'recent' | 'today' | 'yesterday' | 'default'

export type FormattedDate = {
  text: string
  tone: DateTone
}

export type FormatDateOptions = {
  format: DateFormat
  /** Append the local 24-hour time (`HH:mm`) to the absolute value. */
  showTime: boolean
  /** Include seconds in the appended time (`HH:mm:ss`). Only applies when `showTime`. */
  showSeconds: boolean
  /** Render recent timestamps as relative phrases. */
  relative: boolean
  /** Reference epoch (injectable for deterministic tests). */
  now?: number
}

/** Tailwind colour utilities per tone (paired light/dark). */
export const dateToneClassName: Record<DateTone, string> = {
  recent: 'text-accent-green',
  today: 'text-accent-blue-light dark:text-accent-blue',
  yesterday: 'text-accent-amber',
  default: 'text-light-text-muted dark:text-dark-text-muted',
}

const EMPTY: FormattedDate = { text: '—', tone: 'default' }

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
/** Beyond this age the relative phrasing is dropped for the absolute format. */
const RELATIVE_CUTOFF = 2 * DAY

function absolutePattern(format: DateFormat, showTime: boolean, showSeconds: boolean): string {
  const datePattern = datePatterns[format]
  if (!showTime) {
    return datePattern
  }
  return `${datePattern} ${showSeconds ? 'HH:mm:ss' : 'HH:mm'}`
}

function formatRelative(ageMs: number): FormattedDate | null {
  // Clamp clock-skewed "future" timestamps to the present.
  const age = ageMs < 0 ? 0 : ageMs
  if (age >= RELATIVE_CUTOFF) {
    return null
  }
  if (age < MINUTE) {
    return { text: 'just now', tone: 'recent' }
  }
  if (age < HOUR) {
    const minutes = Math.floor(age / MINUTE)
    return { text: `${minutes} minute${minutes === 1 ? '' : 's'} ago`, tone: 'recent' }
  }
  if (age < DAY) {
    const hours = Math.floor(age / HOUR)
    return { text: `${hours} hour${hours === 1 ? '' : 's'} ago`, tone: 'today' }
  }
  const days = Math.floor(age / DAY)
  return { text: `${days} day${days === 1 ? '' : 's'} ago`, tone: 'yesterday' }
}

/**
 * Format a backend timestamp for display.
 *
 * @param value RFC3339 timestamp (or `null` for a missing value).
 * @param options Format, time, and relative-mode preferences.
 */
export function formatEntryDate(
  value: string | null | undefined,
  options: FormatDateOptions,
): FormattedDate {
  if (!value) {
    return EMPTY
  }
  const ms = Date.parse(value)
  if (Number.isNaN(ms)) {
    return EMPTY
  }
  if (options.relative) {
    const phrase = formatRelative((options.now ?? Date.now()) - ms)
    if (phrase) {
      return phrase
    }
  }
  const pattern = absolutePattern(options.format, options.showTime, options.showSeconds)
  return { text: format(new Date(ms), pattern), tone: 'default' }
}
