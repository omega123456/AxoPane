import { useCallback, useEffect, useMemo, useRef, useState } from 'react'
import { ArrowLeftIcon, ArrowRightIcon, RefreshIcon } from '@/components/icons'
import { Button, SelectField } from '@/components/controls'
import { readLogs } from '@/lib/ipc/commands'
import { log } from '@/lib/app-log-commands'
import type { LogDisplayFilter, LogEntry } from '@/lib/types/ipc'
import { formatUtcLogTimestamp } from '@/lib/date-format'
import {
  activeFillClass,
  NO_CLASS,
  SCROLLBAR_THIN,
  spinningClass,
  TEXT_SECONDARY,
} from '@/lib/class-names'
import { lookupOwnProperty } from '@/lib/collections'

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

const BADGE_TONES: Record<string, string> = {
  error:
    'border-light-danger/30 dark:border-dark-danger/30 bg-light-danger/15 dark:bg-dark-danger/15 text-light-danger dark:text-dark-danger',
  warn: 'border-light-warning/30 dark:border-dark-warning/30 bg-light-warning/15 dark:bg-dark-warning/15 text-light-warning dark:text-dark-warning',
  info: 'border-light-accent/30 dark:border-dark-accent/30 bg-light-accent/15 dark:bg-dark-accent/15 text-light-accent dark:text-dark-accent',
  debug: `border-light-divider bg-light-hover ${TEXT_SECONDARY} dark:border-dark-divider dark:bg-dark-hover`,
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

const ROW_TONES: Partial<Record<string, string>> = {
  error: 'bg-light-danger/5 dark:bg-dark-danger/5',
  warn: 'bg-light-warning/5 dark:bg-dark-warning/5',
}

function rowTone(level: string): string {
  return lookupOwnProperty(ROW_TONES, level.toLowerCase()) ?? NO_CLASS
}

type LogLoadState = {
  isLoading: boolean
  isRefreshing: boolean
  hasError: boolean
}

const INITIAL_LOG_LOAD: LogLoadState = { isLoading: true, isRefreshing: false, hasError: false }

function settledLogLoad(hasError: boolean): LogLoadState {
  return { isLoading: false, isRefreshing: false, hasError }
}

function refreshingLogLoad(current: LogLoadState): LogLoadState {
  return { ...current, isRefreshing: true }
}

function useLogEntries() {
  const [entries, setEntries] = useState<LogEntry[]>([])
  const [loadState, setLoadState] = useState(INITIAL_LOG_LOAD)
  const [autoRefresh, setAutoRefresh] = useState(true)
  const requestIdRef = useRef(0)

  const load = useCallback(() => {
    const requestId = ++requestIdRef.current
    const isCurrent = () => requestId === requestIdRef.current
    return readLogs()
      .then((result) => {
        if (!isCurrent()) {
          return
        }
        setEntries(result)
        setLoadState(settledLogLoad(false))
      })
      .catch((error: unknown) => {
        if (!isCurrent()) {
          return
        }
        setLoadState(settledLogLoad(true))
        log.error('failed to read logs', { error: String(error) })
      })
  }, [])

  const refresh = useCallback(() => {
    setLoadState(refreshingLogLoad)
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

  return { entries, loadState, autoRefresh, setAutoRefresh, refresh }
}

function newestAtOrAbove(entries: LogEntry[], filter: LogDisplayFilter) {
  const threshold = FILTER_RANK[filter]
  return entries
    .filter((entry) => (LEVEL_RANK[entry.level.toLowerCase()] ?? LEVEL_RANK.info) >= threshold)
    .slice()
    .reverse()
}

type LogPage = {
  totalPages: number
  safePage: number
  visible: LogEntry[]
  summary: string
}

function logPage(filtered: LogEntry[], page: number): LogPage {
  const totalPages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
  const safePage = Math.min(page, totalPages)
  const pageStart = (safePage - 1) * PAGE_SIZE
  const visible = filtered.slice(pageStart, pageStart + PAGE_SIZE)
  const summary =
    filtered.length === 0
      ? 'No log entries'
      : `${pageStart + 1}-${Math.min(filtered.length, pageStart + visible.length)} of ${filtered.length}`
  return { totalPages, safePage, visible, summary }
}

export function LogViewer() {
  const logs = useLogEntries()
  const [filter, setFilter] = useState<LogDisplayFilter>('all')
  const [page, setPage] = useState(1)
  const filtered = useMemo(() => newestAtOrAbove(logs.entries, filter), [logs.entries, filter])
  const currentPage = logPage(filtered, page)

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
        <LogRefreshControls
          isRefreshing={logs.loadState.isRefreshing}
          autoRefresh={logs.autoRefresh}
          onRefresh={logs.refresh}
          onToggleAutoRefresh={() => logs.setAutoRefresh((value) => !value)}
        />
      </div>
      <LogPager currentPage={currentPage} onPageChange={setPage} />
      <LogBody
        hasError={logs.loadState.hasError}
        isEmpty={!logs.loadState.isLoading && filtered.length === 0}
        visible={currentPage.visible}
      />
    </section>
  )
}

type LogRefreshControlsProps = {
  isRefreshing: boolean
  autoRefresh: boolean
  onRefresh: () => void
  onToggleAutoRefresh: () => void
}

const AUTO_REFRESH_ON_DOT_CLASS = 'bg-light-success dark:bg-dark-success'
const AUTO_REFRESH_OFF_DOT_CLASS = 'bg-light-text-tertiary dark:bg-dark-text-tertiary'

function autoRefreshDotClass(autoRefresh: boolean) {
  return autoRefresh ? AUTO_REFRESH_ON_DOT_CLASS : AUTO_REFRESH_OFF_DOT_CLASS
}

function LogRefreshControls({
  isRefreshing,
  autoRefresh,
  onRefresh,
  onToggleAutoRefresh,
}: LogRefreshControlsProps) {
  return (
    <div className="flex items-center gap-2">
      <Button
        variant="subtle"
        aria-label="Refresh logs"
        title="Refresh logs"
        onClick={onRefresh}
        size="icon"
        data-testid="log-viewer-refresh"
      >
        <RefreshIcon className={`size-4 ${spinningClass(isRefreshing)}`} />
      </Button>
      <Button
        variant="subtle"
        aria-label="Toggle auto-refresh"
        aria-pressed={autoRefresh}
        onClick={onToggleAutoRefresh}
        data-testid="log-viewer-auto-refresh"
        className={activeFillClass(autoRefresh)}
      >
        <span
          aria-hidden="true"
          className={`size-2 rounded-full ${autoRefreshDotClass(autoRefresh)}`}
        />
        {autoRefresh ? 'Auto-refresh on' : 'Auto-refresh off'}
      </Button>
    </div>
  )
}

function LogPager({
  currentPage,
  onPageChange,
}: {
  currentPage: LogPage
  onPageChange: (page: number) => void
}) {
  const { safePage, totalPages, summary } = currentPage
  return (
    <div className="flex items-center justify-between gap-3 border-b border-light-divider px-4 py-1 dark:border-dark-divider">
      <div className="flex items-center gap-1">
        <Button
          variant="subtle"
          aria-label="Previous page"
          onClick={() => onPageChange(Math.max(1, safePage - 1))}
          disabled={safePage <= 1}
          size="icon"
        >
          <ArrowLeftIcon className="size-4" />
        </Button>
        <span className={`min-w-20 text-center text-caption ${TEXT_SECONDARY}`}>
          Page {safePage} / {totalPages}
        </span>
        <Button
          variant="subtle"
          aria-label="Next page"
          onClick={() => onPageChange(Math.min(totalPages, safePage + 1))}
          disabled={safePage >= totalPages}
          size="icon"
        >
          <ArrowRightIcon className="size-4" />
        </Button>
      </div>
      <span className={`text-caption ${TEXT_SECONDARY}`} data-testid="log-viewer-summary">
        {summary}
      </span>
    </div>
  )
}

function LogBody({
  hasError,
  isEmpty,
  visible,
}: {
  hasError: boolean
  isEmpty: boolean
  visible: LogEntry[]
}) {
  if (hasError) {
    return (
      <div
        role="alert"
        data-testid="log-viewer-error"
        className="px-3 py-8 text-center text-row text-light-danger dark:text-dark-danger"
      >
        Failed to load logs.
      </div>
    )
  }
  if (isEmpty) {
    return (
      <div
        data-testid="log-viewer-empty"
        className={`px-3 py-8 text-center text-row ${TEXT_SECONDARY}`}
      >
        No log entries to show.
      </div>
    )
  }
  return (
    <div className={`overflow-auto ${SCROLLBAR_THIN}`}>
      <table className="w-full table-fixed border-collapse">
        <thead>
          <tr className={`text-left text-caption ${TEXT_SECONDARY}`}>
            <th className="w-20 py-2 pl-4 pr-3 font-normal">Level</th>
            <th className="px-3 py-2 font-normal">Message</th>
            <th className="w-32 px-3 py-2 font-normal">Time</th>
          </tr>
        </thead>
        <tbody>
          {visible.map((entry) => (
            <LogRow key={entry.id} entry={entry} />
          ))}
        </tbody>
      </table>
    </div>
  )
}

function LogRow({ entry }: { entry: LogEntry }) {
  return (
    <tr
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
      <td
        className={`whitespace-nowrap px-3 py-2 align-top font-mono text-caption ${TEXT_SECONDARY}`}
      >
        {formatUtcLogTimestamp(entry.timestamp)}
      </td>
    </tr>
  )
}
