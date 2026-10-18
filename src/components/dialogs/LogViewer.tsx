import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeftIcon, ArrowRightIcon, RefreshIcon } from '@/components/icons'
import { Button, SelectField } from '@/components/controls'
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
  error:
    'border-light-danger/30 dark:border-dark-danger/30 bg-light-danger/15 dark:bg-dark-danger/15 text-light-danger dark:text-dark-danger',
  warn: 'border-light-warning/30 dark:border-dark-warning/30 bg-light-warning/15 dark:bg-dark-warning/15 text-light-warning dark:text-dark-warning',
  info: 'border-light-accent/30 dark:border-dark-accent/30 bg-light-accent/15 dark:bg-dark-accent/15 text-light-accent dark:text-dark-accent',
  debug:
    'border-light-divider bg-light-hover text-light-text-secondary dark:border-dark-divider dark:bg-dark-hover dark:text-dark-text-secondary',
  trace:
    'border-light-divider bg-light-card text-light-text-tertiary dark:border-dark-divider dark:bg-dark-card dark:text-dark-text-tertiary',
}

function LogLevelBadge({ level }: { level: string }) {
  const normalized = level.toLowerCase()
  const tone = BADGE_TONES[normalized] ?? BADGE_TONES.info
  return (
    <span
      className={`inline-flex min-w-14 items-center justify-center rounded-full border px-2 py-0.5 font-mono text-caption font-bold uppercase tracking-wide ${tone}`}
    >
      {normalized.toUpperCase()}
    </span>
  )
}

function rowTone(level: string): string {
  const normalized = level.toLowerCase()
  if (normalized === 'error') return 'bg-light-danger/5 dark:bg-dark-danger/5'
  if (normalized === 'warn') return 'bg-light-warning/5 dark:bg-dark-warning/5'
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
      className="mt-1 flex flex-col overflow-hidden rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card"
    >
      <div className="flex items-center justify-between gap-3 border-b border-light-divider px-4 py-2.5 dark:border-dark-divider">
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
          <Button
            variant="subtle"
            aria-label="Refresh logs"
            title="Refresh logs"
            onClick={refresh}
            size="icon"
            data-testid="log-viewer-refresh"
          >
            <RefreshIcon className={`size-4 ${isRefreshing ? 'animate-spin' : ''}`} />
          </Button>
          <Button
            variant="subtle"
            aria-label="Toggle auto-refresh"
            aria-pressed={autoRefresh}
            onClick={() => setAutoRefresh((value) => !value)}
            data-testid="log-viewer-auto-refresh"
            className={autoRefresh ? 'bg-light-hover dark:bg-dark-hover' : ''}
          >
            <span
              aria-hidden="true"
              className={`size-2 rounded-full ${
                autoRefresh
                  ? 'bg-light-success dark:bg-dark-success'
                  : 'bg-light-text-tertiary dark:bg-dark-text-tertiary'
              }`}
            />
            {autoRefresh ? 'Auto-refresh on' : 'Auto-refresh off'}
          </Button>
        </div>
      </div>

      <div className="flex items-center justify-between gap-3 border-b border-light-divider px-4 py-1 dark:border-dark-divider">
        <div className="flex items-center gap-1">
          <Button
            variant="subtle"
            aria-label="Previous page"
            onClick={() => setPage(Math.max(1, safePage - 1))}
            disabled={safePage <= 1}
            size="icon"
          >
            <ArrowLeftIcon className="size-4" />
          </Button>
          <span className="min-w-20 text-center text-caption text-light-text-secondary dark:text-dark-text-secondary">
            Page {safePage} / {totalPages}
          </span>
          <Button
            variant="subtle"
            aria-label="Next page"
            onClick={() => setPage(Math.min(totalPages, safePage + 1))}
            disabled={safePage >= totalPages}
            size="icon"
          >
            <ArrowRightIcon className="size-4" />
          </Button>
        </div>
        <span
          className="text-caption text-light-text-secondary dark:text-dark-text-secondary"
          data-testid="log-viewer-summary"
        >
          {summary}
        </span>
      </div>

      {hasError ? (
        <div
          role="alert"
          data-testid="log-viewer-error"
          className="px-3 py-8 text-center text-row text-light-danger dark:text-dark-danger"
        >
          Failed to load logs.
        </div>
      ) : !isLoading && filtered.length === 0 ? (
        <div
          data-testid="log-viewer-empty"
          className="px-3 py-8 text-center text-row text-light-text-secondary dark:text-dark-text-secondary"
        >
          No log entries to show.
        </div>
      ) : (
        <div className="overflow-auto scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary">
          <table className="w-full table-fixed border-collapse">
            <thead>
              <tr className="text-left text-caption text-light-text-secondary dark:text-dark-text-secondary">
                <th className="w-20 py-2 pl-4 pr-3 font-normal">Level</th>
                <th className="px-3 py-2 font-normal">Message</th>
                <th className="w-32 px-3 py-2 font-normal">Time</th>
              </tr>
            </thead>
            <tbody>
              {visible.map((entry) => (
                <tr
                  key={entry.id}
                  className={`border-b border-light-divider dark:border-dark-divider ${rowTone(entry.level)}`}
                >
                  <td className="py-2 pl-4 pr-3 align-top">
                    <LogLevelBadge level={entry.level} />
                  </td>
                  <td className="px-3 py-2 align-top">
                    <span
                      title={entry.message}
                      className="block truncate font-mono text-caption text-light-text dark:text-dark-text"
                      data-testid={`log-viewer-message-${entry.id}`}
                    >
                      {entry.message}
                    </span>
                  </td>
                  <td className="whitespace-nowrap px-3 py-2 align-top font-mono text-caption text-light-text-secondary dark:text-dark-text-secondary">
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
