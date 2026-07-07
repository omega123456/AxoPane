import { format, getTime, isValid, parseISO } from 'date-fns'

/**
 * Date formatting for directory listings and the properties dialog.
 *
 * Backend timestamps arrive as RFC3339 strings. Absolute values are rendered in
 * the user's local timezone via `date-fns` (the `format` helper localises the
 * instant and supplies ordinals through the `do` token). The time portion is
 * appended only when `showTime` is set — with seconds only when `showSeconds`
 * is also set — so it composes with any date format rather than multiplying the
 * catalogue. The optional relative mode replaces the absolute value with
 * phrases like `15 minutes ago`, keeps `1/2/3 days ago` for the first few day
 * buckets, then switches to weekday labels like `on Tuesday` for older items
 * from the past week before falling back to the chosen absolute format.
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

/**
 * Absolute-format results are pure functions of `value|pattern` (no volatile
 * "now" input), so repeated cells sharing a value/format — the common case
 * while scrolling a folder without touching the clock — reuse the same
 * formatted result instead of re-running `date-fns` `format` every render.
 * Capped so a session that pages through many distinct folders can't grow
 * this unbounded.
 */
const ABSOLUTE_FORMAT_CACHE_LIMIT = 2000
const absoluteFormatCache = new Map<string, FormattedDate>()

function cachedAbsoluteFormat(value: string, pattern: string, compute: () => FormattedDate): FormattedDate {
  const cacheKey = `${value}|${pattern}`
  const cached = absoluteFormatCache.get(cacheKey)
  if (cached) {
    return cached
  }

  const result = compute()
  if (absoluteFormatCache.size >= ABSOLUTE_FORMAT_CACHE_LIMIT) {
    // Evict the oldest inserted entry (Map preserves insertion order) rather
    // than tracking recency — simple and good enough for a soft size cap.
    const oldestKey = absoluteFormatCache.keys().next().value
    if (oldestKey !== undefined) {
      absoluteFormatCache.delete(oldestKey)
    }
  }
  absoluteFormatCache.set(cacheKey, result)
  return result
}

const MINUTE = 60_000
const HOUR = 60 * MINUTE
const DAY = 24 * HOUR
const RELATIVE_DAY_PRIORITY_CUTOFF = 4 * DAY
/** Beyond this age the relative phrasing is dropped for the absolute format. */
const RELATIVE_CUTOFF = 7 * DAY

function absolutePattern(format: DateFormat, showTime: boolean, showSeconds: boolean): string {
  const datePattern = datePatterns[format]
  if (!showTime) {
    return datePattern
  }
  return `${datePattern} ${showSeconds ? 'HH:mm:ss' : 'HH:mm'}`
}

function relativeWeekdayPattern(showTime: boolean, showSeconds: boolean): string {
  if (!showTime) {
    return "'on' EEEE"
  }
  return `'on' EEEE ${showSeconds ? 'HH:mm:ss' : 'HH:mm'}`
}

function formatRelative(value: Date, ageMs: number, showTime: boolean, showSeconds: boolean): FormattedDate | null {
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
  if (age < RELATIVE_DAY_PRIORITY_CUTOFF) {
    const days = Math.floor(age / DAY)
    return { text: `${days} day${days === 1 ? '' : 's'} ago`, tone: 'yesterday' }
  }
  return { text: format(value, relativeWeekdayPattern(showTime, showSeconds)), tone: 'default' }
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
  const date = parseISO(value)
  if (!isValid(date)) {
    return EMPTY
  }
  const ms = getTime(date)
  if (options.relative) {
    const phrase = formatRelative(
      date,
      (options.now ?? Date.now()) - ms,
      options.showTime,
      options.showSeconds,
    )
    if (phrase) {
      // Relative phrases are a function of "now", which advances outside of
      // any cache key we could construct — never cache these.
      return phrase
    }
  }
  const pattern = absolutePattern(options.format, options.showTime, options.showSeconds)
  return cachedAbsoluteFormat(value, pattern, () => ({
    text: format(date, pattern),
    tone: 'default',
  }))
}
