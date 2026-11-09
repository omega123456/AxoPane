import { UTCDate } from '@date-fns/utc'
import { format, getTime, isValid, parseISO } from 'date-fns'
import {
  millisecondsInDay,
  millisecondsInHour,
  millisecondsInMinute,
  millisecondsInWeek,
} from 'date-fns/constants'
import { TEXT_SECONDARY } from '@/lib/class-names'
import { EMPTY_VALUE } from '@/lib/format'

export const DATE_FORMATS = ['ymd', 'dmy', 'mdy', 'med', 'dme', 'lmd', 'dml', 'dly'] as const

export type DateFormat = (typeof DATE_FORMATS)[number]

export const DEFAULT_DATE_FORMAT: DateFormat = 'ymd'

export function isDateFormat(value: string): value is DateFormat {
  return (DATE_FORMATS as readonly string[]).includes(value)
}

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

export type DateTone = 'recent' | 'today' | 'yesterday' | 'default'

export type FormattedDate = {
  text: string
  tone: DateTone
}

export type FormatDateOptions = {
  format: DateFormat
  showTime: boolean
  showSeconds: boolean
  relative: boolean
  now?: number
}

export const dateToneClassName: Record<DateTone, string> = {
  recent: 'text-light-success dark:text-dark-success',
  today: 'text-light-accent dark:text-dark-accent',
  yesterday: 'text-light-warning dark:text-dark-warning',
  default: TEXT_SECONDARY,
}

const EMPTY: FormattedDate = { text: EMPTY_VALUE, tone: 'default' }

const ABSOLUTE_FORMAT_CACHE_LIMIT = 2000
const absoluteFormatCache = new Map<string, FormattedDate>()

function cachedAbsoluteFormat(
  value: string,
  pattern: string,
  compute: () => FormattedDate,
): FormattedDate {
  const cacheKey = `${value}|${pattern}`
  const cached = absoluteFormatCache.get(cacheKey)
  if (cached) {
    return cached
  }

  const result = compute()
  if (absoluteFormatCache.size >= ABSOLUTE_FORMAT_CACHE_LIMIT) {
    const oldestKey = absoluteFormatCache.keys().next().value
    if (oldestKey !== undefined) {
      absoluteFormatCache.delete(oldestKey)
    }
  }
  absoluteFormatCache.set(cacheKey, result)
  return result
}

const MINUTE = millisecondsInMinute
const HOUR = millisecondsInHour
const DAY = millisecondsInDay
const RELATIVE_DAY_PRIORITY_CUTOFF_DAYS = 4
const RELATIVE_DAY_PRIORITY_CUTOFF = RELATIVE_DAY_PRIORITY_CUTOFF_DAYS * DAY
const RELATIVE_CUTOFF = millisecondsInWeek

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

type TimeDisplay = Pick<FormatDateOptions, 'showTime' | 'showSeconds'>

function formatRelative(
  value: Date,
  ageMs: number,
  { showTime, showSeconds }: TimeDisplay,
): FormattedDate | null {
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
  const timestamp = getTime(date)
  if (options.relative) {
    const phrase = formatRelative(date, (options.now ?? Date.now()) - timestamp, options)
    if (phrase) {
      return phrase
    }
  }
  const pattern = absolutePattern(options.format, options.showTime, options.showSeconds)
  return cachedAbsoluteFormat(value, pattern, () => ({
    text: format(date, pattern),
    tone: 'default',
  }))
}

const LOG_TIMESTAMP_PATTERN = 'MMM dd HH:mm:ss'

export function formatUtcLogTimestamp(iso: string): string {
  const date = new UTCDate(iso)
  return isValid(date) ? format(date, LOG_TIMESTAMP_PATTERN) : iso
}
