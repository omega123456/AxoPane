import { useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  ArrowRightIcon,
  CheckCircleIcon,
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  SkipForwardIcon,
  XCircleIcon,
  XIcon,
} from '@/components/icons'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import { formatBytes, formatCount, formatEta, formatRate } from '@/lib/format'
import type { OpProgress, ThroughputSample } from '@/lib/types/ipc'

type JobCardProps = {
  operation: OpProgress
  throughputHistory: ThroughputSample[]
  throughputPeak: number
  hasConflict: boolean
  reorderable: boolean
  onPause: () => void
  onResume: () => void
  onCancel: () => void
  onDismiss: () => void
  onSkip: () => void
  onRetry: () => void
  onResolve: () => void
  onMoveUp?: () => void
  onMoveDown?: () => void
}

const LIVE_REGION_THROTTLE_MS = 5000
/**
 * Cadence the visible metrics row refreshes at. The backend reports a jittery
 * instantaneous rate many times a second; sampling it on a calm interval keeps
 * the speed number readable (and roughly in step with the averaged chart)
 * instead of flickering on every progress event.
 */
const METRICS_REFRESH_MS = 500

function verb(operation: OpProgress) {
  return operation.kind === 'move' ? 'Moving' : 'Copying'
}

function itemSummary(itemNames: string[]) {
  if (itemNames.length === 0) {
    return null
  }
  if (itemNames.length <= 2) {
    return itemNames.join(', ')
  }
  return `${itemNames.slice(0, 2).join(', ')}, +${formatCount(itemNames.length - 2)} more`
}

export function JobCard({
  operation,
  throughputHistory,
  throughputPeak,
  hasConflict,
  reorderable,
  onPause,
  onResume,
  onCancel,
  onDismiss,
  onSkip,
  onRetry,
  onResolve,
  onMoveUp,
  onMoveDown,
}: JobCardProps) {
  const headingId = useId()
  const percent = Math.min(100, Math.max(0, operation.progressPercent))
  const roundedPercent = Math.round(percent)
  const filePercent =
    operation.currentFileTotalBytes > 0
      ? Math.min(
          100,
          (operation.currentFileCopiedBytes / operation.currentFileTotalBytes) * 100,
        )
      : 0
  const isCompleted = operation.status === 'completed'
  const isFailed = operation.status === 'failed'
  const isPaused = operation.status === 'paused'
  const isCancelled = operation.status === 'cancelled'
  const isConflict = operation.status === 'conflict' || hasConflict
  const isPending = operation.status === 'pending'
  const queuedItems = itemSummary(operation.itemNames)
  const showQueuedItems = isPending && queuedItems !== null
  const showChart = !isPending && !isPaused && !isCompleted && !isFailed && !isCancelled
  // Show the chart's smoothed leading-edge rate (not the raw instantaneous one)
  // so the number matches the curve and stays calm; fall back before any history.
  const currentRate =
    throughputHistory.length > 0
      ? throughputHistory[throughputHistory.length - 1].rate
      : operation.bytesPerSecond
  const metrics = useMemo(
    () => ({
      rate: formatRate(currentRate),
      eta:
        operation.etaSeconds !== null ? formatEta(operation.etaSeconds) : 'estimating…',
      items: `${formatCount(operation.completedItems)} / ${formatCount(operation.totalItems)} items`,
    }),
    [currentRate, operation.completedItems, operation.etaSeconds, operation.totalItems],
  )
  // Throttle the live view (metrics + chart) to a calm, fixed cadence so the
  // number and the chart's leading edge update together a few times per second
  // — like the Windows copy dialog — rather than on every backend event.
  const liveView = useMemo(
    () => ({ metrics, samples: throughputHistory, percent, peak: throughputPeak }),
    [metrics, throughputHistory, percent, throughputPeak],
  )
  const [view, setView] = useState(liveView)
  const latestViewRef = useRef(liveView)
  useEffect(() => {
    latestViewRef.current = liveView
  })
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      setView(latestViewRef.current)
    }, METRICS_REFRESH_MS)
    return () => window.clearInterval(intervalId)
  }, [])
  const displayMetrics = view.metrics

  const metricsAnnouncement = `${metrics.rate}, ${metrics.eta}, ${metrics.items}`
  const [liveMetricsAnnouncement, setLiveMetricsAnnouncement] = useState(metricsAnnouncement)
  const liveMetricsAnnouncementRef = useRef(liveMetricsAnnouncement)
  const pendingLiveAnnouncementRef = useRef<string | null>(null)
  const liveRegionTimerRef = useRef<number | null>(null)

  useEffect(() => {
    liveMetricsAnnouncementRef.current = liveMetricsAnnouncement
  }, [liveMetricsAnnouncement])

  useEffect(() => {
    if (metricsAnnouncement === liveMetricsAnnouncement) {
      pendingLiveAnnouncementRef.current = null
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
        liveRegionTimerRef.current = null
      }
      return
    }

    pendingLiveAnnouncementRef.current = metricsAnnouncement
    if (liveRegionTimerRef.current !== null) {
      return
    }

    liveRegionTimerRef.current = window.setTimeout(() => {
      liveRegionTimerRef.current = null
      const nextAnnouncement = pendingLiveAnnouncementRef.current
      pendingLiveAnnouncementRef.current = null
      if (
        nextAnnouncement !== null &&
        nextAnnouncement !== liveMetricsAnnouncementRef.current
      ) {
        setLiveMetricsAnnouncement(nextAnnouncement)
        liveMetricsAnnouncementRef.current = nextAnnouncement
      }
    }, LIVE_REGION_THROTTLE_MS)
  }, [liveMetricsAnnouncement, metricsAnnouncement])

  useEffect(() => {
    return () => {
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
      }
    }
  }, [])

  return (
    <article
      aria-labelledby={headingId}
      data-status={operation.status}
      className="border-b border-light-border p-4 last:border-b-0 dark:border-dark-border"
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex min-w-0 items-start gap-2">
          {reorderable ? (
            <span className="mt-0.5 flex shrink-0 flex-col">
              <button
                type="button"
                aria-label="Move job up"
                onClick={onMoveUp}
                disabled={!onMoveUp}
                className="flex h-4 w-4 items-center justify-center rounded text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover disabled:opacity-40 dark:text-dark-text-muted dark:hover:bg-dark-hover"
              >
                <GripVerticalIcon className="h-3.5 w-3.5" />
              </button>
            </span>
          ) : null}
          <div className="min-w-0">
            <div
              id={headingId}
              className="flex items-center gap-1.5 text-sm font-semibold text-light-text dark:text-dark-text"
            >
              {isCompleted ? (
                <CheckCircleIcon className="h-4 w-4 shrink-0 text-accent-green" />
              ) : null}
              {isFailed ? (
                <XCircleIcon className="h-4 w-4 shrink-0 text-accent-red" />
              ) : null}
              {isCancelled ? (
                <XCircleIcon className="h-4 w-4 shrink-0 text-light-text-muted dark:text-dark-text-muted" />
              ) : null}
              <span className="truncate">
                {isCompleted
                  ? `${verb(operation)} complete`
                  : isFailed
                    ? `${verb(operation)} failed`
                    : isCancelled
                      ? `${verb(operation)} cancelled`
                    : `${verb(operation)} ${formatCount(operation.totalItems)} items`}
              </span>
            </div>
            <div className="mt-1 truncate font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
              {showQueuedItems ? (
                <span className="mb-1 block truncate text-light-text-soft dark:text-dark-text-soft">
                  {queuedItems}
                </span>
              ) : null}
              {operation.sourceDir}{' '}
              <ArrowRightIcon className="inline h-3 w-3 align-[-2px] text-accent-blue-light dark:text-accent-blue" />{' '}
              {operation.destinationDir}
            </div>
          </div>
        </div>
        <span
          className={`shrink-0 font-mono text-2xl font-semibold leading-none ${
            isFailed ? 'text-accent-red' : 'text-accent-blue-light dark:text-accent-blue'
          }`}
        >
          {roundedPercent}%
        </span>
      </div>

      <div className="mt-2">
        <span className="sr-only" aria-live="polite" aria-atomic="true">
          {liveMetricsAnnouncement}
        </span>
        <div className="flex flex-wrap items-center gap-2 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
          <span className="text-light-text-soft dark:text-dark-text-soft">{displayMetrics.rate}</span>
          <span className="text-light-text-faint dark:text-dark-text-faint">·</span>
          <span>{displayMetrics.eta}</span>
          <span className="text-light-text-faint dark:text-dark-text-faint">·</span>
          <span>{displayMetrics.items}</span>
        </div>
      </div>

      {operation.currentFileName ? (
        <div className="mt-3.5 border-t border-light-border pt-3 dark:border-dark-border">
          <div className="flex items-center justify-between gap-3">
            <span className="truncate text-xs text-light-text dark:text-dark-text">
              {operation.currentFileName}
            </span>
            <span className="shrink-0 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
              {formatBytes(operation.currentFileCopiedBytes)} /{' '}
              {formatBytes(operation.currentFileTotalBytes)}
            </span>
          </div>
          <div
            role="progressbar"
            aria-label={`Current file progress for ${operation.currentFileName}`}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={Math.round(filePercent)}
            className="mt-2 h-1 overflow-hidden rounded-full bg-light-skeleton dark:bg-dark-skeleton"
          >
            <div
              className="h-full rounded-full bg-accent-blue-light opacity-70 dark:bg-accent-blue"
              style={{ width: `${filePercent}%` }}
            />
          </div>
        </div>
      ) : null}

      {isFailed && operation.errorMessage ? (
        <div className="mt-3 rounded-tab bg-accent-red-soft px-3 py-2 text-uxs text-accent-red">
          {operation.errorMessage}
        </div>
      ) : isCancelled ? (
        <div className="mt-3 rounded-tab border border-light-border px-3 py-2 text-uxs text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
          Transfer cancelled. Already-copied files were kept.
        </div>
      ) : showChart ? (
        <div
          role="progressbar"
          aria-labelledby={headingId}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={roundedPercent}
          className="mt-3.5"
        >
          <ThroughputChart
            samples={view.samples}
            currentPercent={view.percent}
            peakRate={view.peak}
          />
        </div>
      ) : null}

      <div className="mt-4 flex flex-wrap items-center gap-2">
        {isConflict ? (
          <button
            type="button"
            onClick={onResolve}
            className="flex items-center gap-1.5 rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            Resolve conflict
          </button>
        ) : isFailed ? (
          <>
            <button
              type="button"
              onClick={onRetry}
              className="flex items-center gap-1.5 rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
            >
              <RotateCcwIcon className="h-3.5 w-3.5" /> Retry
            </button>
            <button
              type="button"
              onClick={onDismiss}
              className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
            >
              <XIcon className="h-3.5 w-3.5" /> Dismiss
            </button>
          </>
        ) : isCompleted || isCancelled ? (
          <button
            type="button"
            onClick={onDismiss}
            className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            <XIcon className="h-3.5 w-3.5" /> Dismiss
          </button>
        ) : isPaused ? (
          <button
            type="button"
            onClick={onResume}
            className="flex items-center gap-1.5 rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <PlayIcon className="h-3.5 w-3.5" /> Resume
          </button>
        ) : (
          <button
            type="button"
            onClick={onPause}
            className="flex items-center gap-1.5 rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <PauseIcon className="h-3.5 w-3.5" /> Pause
          </button>
        )}

        {!isCompleted && !isFailed && !isCancelled ? (
          <button
            type="button"
            onClick={onSkip}
            className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            <SkipForwardIcon className="h-3.5 w-3.5" /> Skip
          </button>
        ) : null}

        {!isCompleted && !isFailed && !isCancelled ? (
          <button
            type="button"
            onClick={onCancel}
            className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            <XIcon className="h-3.5 w-3.5" /> Cancel
          </button>
        ) : null}

        {reorderable && onMoveDown ? (
          <button
            type="button"
            aria-label="Move job down"
            onClick={onMoveDown}
            className="ml-auto flex h-7 items-center rounded-md px-2 text-uxs text-light-text-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:text-dark-text-muted dark:hover:bg-dark-hover"
          >
            Move down
          </button>
        ) : null}
      </div>
    </article>
  )
}
