import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeftIcon, ArrowRightIcon, RefreshIcon } from '@/components/icons'
import { SelectField } from '@/components/controls'
import { readLogs } from '@/lib/ipc/commands'
import { log } from '@/lib/app-log-commands'
import type { LogDisplayFilter, LogEntry } from '@/lib/types/ipc'

const PAGE_SIZE = 20
const AUTO_REFRESH_MS = 5000

const FILTER_OPTIONS: { value: LogDisplayFilter; label: string }[] = [
  { value: 'all', label: 'All' },
  { value: 'error', label: 'Error+' },
  { value: 'warn', label: 'Warn+' },
  { value: 'info', label: 'Info+' },
  { value: 'debug', label: 'Debug+' },
  { value: 'trace', label: 'Trace' },
]

const LEVEL_RANK: Record<string, number> = {
  error: 4,
  warn: 3,
  info: 2,
  debug: 1,
  trace: 0,
}

const FILTER_RANK: Record<LogDisplayFilter, number> = {
  all: -1,
  trace: 0,
  debug: 1,
  info: 2,
  warn: 3,
  error: 4,
}

const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec']

/** Format an ISO timestamp as `Jun 30 12:00:00` in UTC (deterministic). */
function formatTimestamp(iso: string): string {
  const date = new Date(iso)
  if (Number.isNaN(date.getTime())) {
    return iso
  }
  const pad = (value: number) => value.toString().padStart(2, '0')
  return (
    `${MONTHS[date.getUTCMonth()]} ${pad(date.getUTCDate())} ` +
    `${pad(date.getUTCHours())}:${pad(date.getUTCMinutes())}:${pad(date.getUTCSeconds())}`
  )
}

const BADGE_TONES: Record<string, string> = {
  error: 'border-accent-red/30 bg-accent-red/15 text-accent-red',
  warn: 'border-accent-amber/30 bg-accent-amber/15 text-accent-amber',
  info: 'border-accent-blue/30 bg-accent-blue/15 text-accent-blue-light dark:text-accent-blue',
  debug:
    'border-light-border bg-light-hover text-light-text-muted dark:border-dark-border dark:bg-dark-hover dark:text-dark-text-muted',
  trace:
    'border-light-border bg-light-surface text-light-text-faint dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-faint',
}

function LogLevelBadge({ level }: { level: string }) {
  const normalized = level.toLowerCase()
  const tone = BADGE_TONES[normalized] ?? BADGE_TONES.info
  return (
    <span
      className={`inline-flex min-w-14 items-center justify-center rounded-full border px-2 py-0.5 font-mono text-2xs font-bold uppercase tracking-wide ${tone}`}
    >
      {normalized.toUpperCase()}
    </span>
  )
}

function rowTone(level: string): string {
  const normalized = level.toLowerCase()
  if (normalized === 'error') return 'bg-accent-red/5'
  if (normalized === 'warn') return 'bg-accent-amber/5'
  return ''
}

export function LogViewer() {
  const [entries, setEntries] = useState<LogEntry[]>([])
  const [filter, setFilter] = useState<LogDisplayFilter>('all')
  const [page, setPage] = useState(1)
  const [isLoading, setIsLoading] = useState(true)
  const [isRefreshing, setIsRefreshing] = useState(false)
  const [hasError, setHasError] = useState(false)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const requestIdRef = useRef(0)

  // State updates live in deferred promise callbacks (never synchronously), so
  // this is safe to invoke from an effect.
  const load = useCallback(() => {
    const requestId = ++requestIdRef.current
    return readLogs()
      .then((result) => {
        if (requestId !== requestIdRef.current) {
          return
        }
        setEntries(result)
        setHasError(false)
      })
      .catch((error: unknown) => {
        if (requestId !== requestIdRef.current) {
          return
        }
        setHasError(true)
        log.error('failed to read logs', { error: String(error) })
      })
      .finally(() => {
        if (requestId === requestIdRef.current) {
          setIsLoading(false)
          setIsRefreshing(false)
        }
      })
  }, [])

  const refresh = useCallback(() => {
    setIsRefreshing(true)
    void load()
  }, [load])

  useEffect(() => {
    void load()
  }, [load])

  useEffect(() => {
    if (!autoRefresh) {
      return
    }
    const intervalId = window.setInterval(refresh, AUTO_REFRESH_MS)
    return () => window.clearInterval(intervalId)
  }, [autoRefresh, refresh])

  const filtered = useMemo(() => {
    const threshold = FILTER_RANK[filter]
    // Newest first, then apply the severity floor.
    return entries
      .filter((entry) => (LEVEL_RANK[entry.level.toLowerCase()] ?? 2) >= threshold)
      .slice()
      .reverse()
  }, [entries, filter])

  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const safePage = Math.min(page, totalPages)
  const pageStart = (safePage - 1) * PAGE_SIZE
  const visible = filtered.slice(pageStart, pageStart + PAGE_SIZE)
  const summary =
    filtered.length === 0
      ? 'No log entries'
      : `${pageStart + 1}-${Math.min(filtered.length, pageStart + visible.length)} of ${filtered.length}`

  return (
    <section
      aria-label="Application logs"
      data-testid="log-viewer"
      className="mt-4 flex flex-col overflow-hidden rounded-modal border border-light-border bg-light-panel dark:border-dark-border dark:bg-dark-panel"
    >
      <div className="flex items-center justify-between gap-3 border-b border-light-border bg-light-surface p-3 dark:border-dark-border dark:bg-dark-surface">
        <SelectField
          ariaLabel="Severity filter"
          value={filter}
          onChange={(value) => {
            setPage(1)
            setFilter(value)
          }}
          options={FILTER_OPTIONS}
        />
        <div className="flex items-center gap-2">
          <button
            type="button"
            aria-label="Refresh logs"
            title="Refresh logs"
            onClick={refresh}
            className="flex size-8 cursor-pointer items-center justify-center rounded-md text-light-text-muted hover:bg-light-hover hover:text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-muted dark:hover:bg-dark-hover dark:hover:text-accent-blue"
            data-testid="log-viewer-refresh"
          >
            <RefreshIcon className={`size-4 ${isRefreshing ? 'animate-spin' : ''}`} />
          </button>
          <button
            type="button"
            aria-label="Toggle auto-refresh"
            aria-pressed={autoRefresh}
            onClick={() => setAutoRefresh((value) => !value)}
            data-testid="log-viewer-auto-refresh"
            className={`inline-flex cursor-pointer items-center gap-1.5 rounded-full border px-2.5 py-1 text-uxs font-semibold focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
              autoRefresh
                ? 'border-accent-green/40 text-accent-green'
                : 'border-light-border text-light-text-muted dark:border-dark-border dark:text-dark-text-muted'
            }`}
          >
            <span
              aria-hidden="true"
              className={`size-2 rounded-full ${
                autoRefresh
                  ? 'animate-pulse bg-accent-green'
                  : 'bg-light-text-faint dark:bg-dark-text-faint'
              }`}
            />
            {autoRefresh ? 'Auto-refresh on' : 'Auto-refresh off'}
          </button>
        </div>
      </div>

      <div className="flex items-center justify-between gap-3 border-b border-light-border bg-light-surface px-3 py-2 dark:border-dark-border dark:bg-dark-surface">
        <div className="flex items-center gap-1">
          <button
            type="button"
            aria-label="Previous page"
            onClick={() => setPage(Math.max(1, safePage - 1))}
            disabled={safePage <= 1}
            className="flex size-7 cursor-pointer items-center justify-center rounded-md text-light-text-muted hover:bg-light-hover disabled:cursor-default disabled:opacity-40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <ArrowLeftIcon className="size-4" />
          </button>
          <span className="min-w-20 text-center text-uxs text-light-text-muted dark:text-dark-text-muted">
            Page {safePage} / {totalPages}
          </span>
          <button
            type="button"
            aria-label="Next page"
            onClick={() => setPage(Math.min(totalPages, safePage + 1))}
            disabled={safePage >= totalPages}
            className="flex size-7 cursor-pointer items-center justify-center rounded-md text-light-text-muted hover:bg-light-hover disabled:cursor-default disabled:opacity-40 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            <ArrowRightIcon className="size-4" />
          </button>
        </div>
        <span
          className="text-uxs text-light-text-muted dark:text-dark-text-muted"
          data-testid="log-viewer-summary"
        >
          {summary}
        </span>
      </div>

      {hasError ? (
        <div
          role="alert"
          data-testid="log-viewer-error"
          className="px-3 py-8 text-center text-row text-accent-red"
        >
          Failed to load logs.
        </div>
      ) : !isLoading && filtered.length === 0 ? (
        <div
          data-testid="log-viewer-empty"
          className="px-3 py-8 text-center text-row text-light-text-muted dark:text-dark-text-muted"
        >
          No log entries to show.
        </div>
      ) : (
        <div className="overflow-auto">
          <table className="w-full table-fixed border-collapse">
            <thead className="bg-light-titlebar dark:bg-dark-titlebar">
              <tr className="text-left text-2xs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
                <th className="w-20 px-3 py-2 font-bold">Level</th>
                <th className="px-3 py-2 font-bold">Message</th>
                <th className="w-32 px-3 py-2 font-bold">Time</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((entry) => (
                <tr
                  key={entry.id}
                  className={`border-b border-light-border dark:border-dark-border ${rowTone(entry.level)}`}
                >
                  <td className="px-3 py-2 align-top">
                    <LogLevelBadge level={entry.level} />
                  </td>
                  <td className="px-3 py-2 align-top">
                    <span
                      title={entry.message}
                      className="block truncate font-mono text-uxs text-light-text dark:text-dark-text"
                      data-testid={`log-viewer-message-${entry.id}`}
                    >
                      {entry.message}
                    </span>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 align-top font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
                    {formatTimestamp(entry.timestamp)}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  )
}
