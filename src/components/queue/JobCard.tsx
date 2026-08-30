import { useEffect, useId, useMemo, useRef, useState } from 'react'
import {
  ArrowRightIcon,
  CheckIcon,
  CheckCircleIcon,
  ChevronDownIcon,
  ChevronUpIcon,
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  SkipForwardIcon,
  XCircleIcon,
  XIcon,
} from '@/components/icons'
import { ThroughputChart } from '@/components/queue/ThroughputChart'
import { formatCount, formatRate } from '@/lib/format'
import { ITEM_PREVIEW_LIMIT, formatJobProgress, verb } from '@/lib/queue-format'
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
 * Cadence the visible metrics refresh at. The backend reports a jittery
 * instantaneous rate many times a second; sampling it on a calm interval keeps
 * the numbers readable (and roughly in step with the averaged chart) instead of
 * flickering on every progress event. The same tick decides whether the backend
 * is still discovering bytes, by comparing successive totals.
 */
const METRICS_REFRESH_MS = 500

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
  const itemListId = useId()
  const percent = Math.min(100, Math.max(0, operation.progressPercent))
  const roundedPercent = Math.round(percent)
  const isCompleted = operation.status === 'completed'
  const isFailed = operation.status === 'failed'
  const isPaused = operation.status === 'paused'
  const isCancelled = operation.status === 'cancelled'
  const isConflict = operation.status === 'conflict' || hasConflict
  const isPending = operation.status === 'pending'
  const isFinished = isCompleted || isFailed || isCancelled
  const showProgress = !isFinished
  const showCurrentFile = !isPending && !isFinished
  // A delete is measured in items, so a transfer rate and a throughput curve
  // would describe something the rest of the card does not report.
  const isDelete = operation.kind === 'delete'
  const showChart = showCurrentFile && !isDelete
  const showRate = showCurrentFile && !isDelete

  const previewNames = operation.itemNames.slice(0, ITEM_PREVIEW_LIMIT)
  const hiddenItemCount = Math.max(0, operation.totalItems - previewNames.length)
  const listedNames = operation.itemNames
  // The backend sends a bounded preview, so a very large selection can still
  // have names beyond the ones it shipped. Say how many are unlisted rather
  // than implying the list is complete.
  const unlistedCount = Math.max(0, operation.totalItems - listedNames.length)
  const [itemsExpanded, setItemsExpanded] = useState(false)

  // The backend briefly clears the current-file fields between finishing one
  // file and starting the next, which would otherwise unmount this whole
  // block and make the card's height (and everything below it) jump. Freeze
  // the last known file in state and keep showing it through that gap; the
  // block itself stays mounted for the whole active-ish lifetime of the job
  // so its reserved space never collapses mid-run. (React's documented
  // "adjusting state during render" pattern — the guarded `setState` below
  // only fires when the snapshot actually changed, so it settles in the same
  // render pass instead of looping.)
  const [lastCurrentFile, setLastCurrentFile] = useState(operation.currentFileName)
  if (operation.currentFileName && operation.currentFileName !== lastCurrentFile) {
    setLastCurrentFile(operation.currentFileName)
  }
  const displayedFileName = showCurrentFile ? lastCurrentFile : null

  // Show the chart's smoothed leading-edge rate (not the raw instantaneous one)
  // so the number matches the curve and stays calm; fall back before any history.
  const currentRate =
    throughputHistory.length > 0
      ? throughputHistory[throughputHistory.length - 1].rate
      : operation.bytesPerSecond
  const liveView = useMemo(
    () => ({ operation, rate: currentRate, samples: throughputHistory, peak: throughputPeak }),
    [currentRate, operation, throughputHistory, throughputPeak],
  )
  // Throttle the live view (numbers + chart) to a calm, fixed cadence so they
  // update together a few times per second — like the Windows copy dialog —
  // rather than on every backend event.
  const [view, setView] = useState({ ...liveView, scanning: false })
  const latestViewRef = useRef(liveView)
  const lastTotalBytesRef = useRef(operation.totalBytes)
  useEffect(() => {
    latestViewRef.current = liveView
  })
  useEffect(() => {
    const intervalId = window.setInterval(() => {
      const latest = latestViewRef.current
      const scanning = latest.operation.totalBytes > lastTotalBytesRef.current
      lastTotalBytesRef.current = latest.operation.totalBytes
      setView({ ...latest, scanning })
    }, METRICS_REFRESH_MS)
    return () => window.clearInterval(intervalId)
  }, [])

  const displayed = view.operation
  // While a job is actively working through its queue, "N completed" reads as
  // "item 0 of 2" for the entire time the first item is copying — show the item
  // currently being worked on instead (like Windows' "item 1 of 2"). Once the
  // job stops (done/failed/cancelled), fall back to the true completed count.
  const displayedItemNumber = isFinished
    ? displayed.completedItems
    : Math.min(displayed.completedItems + 1, displayed.totalItems)
  const progressSegments = useMemo(
    () => formatJobProgress(displayed, { scanning: view.scanning }),
    [displayed, view.scanning],
  )
  const summary = progressSegments.join(' · ')
  const itemIndex = `item ${formatCount(displayedItemNumber)} of ${formatCount(displayed.totalItems)}`
  const rate = isPaused ? '—' : formatRate(view.rate)

  const announcement = progressSegments.join(', ')
  const [liveAnnouncement, setLiveAnnouncement] = useState(announcement)
  const liveAnnouncementRef = useRef(liveAnnouncement)
  const pendingLiveAnnouncementRef = useRef<string | null>(null)
  const liveRegionTimerRef = useRef<number | null>(null)

  useEffect(() => {
    liveAnnouncementRef.current = liveAnnouncement
  }, [liveAnnouncement])

  useEffect(() => {
    if (announcement === liveAnnouncement) {
      pendingLiveAnnouncementRef.current = null
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
        liveRegionTimerRef.current = null
      }
      return
    }

    pendingLiveAnnouncementRef.current = announcement
    if (liveRegionTimerRef.current !== null) {
      return
    }

    liveRegionTimerRef.current = window.setTimeout(() => {
      liveRegionTimerRef.current = null
      const nextAnnouncement = pendingLiveAnnouncementRef.current
      pendingLiveAnnouncementRef.current = null
      if (nextAnnouncement !== null && nextAnnouncement !== liveAnnouncementRef.current) {
        setLiveAnnouncement(nextAnnouncement)
        liveAnnouncementRef.current = nextAnnouncement
      }
    }, LIVE_REGION_THROTTLE_MS)
  }, [announcement, liveAnnouncement])

  useEffect(() => {
    return () => {
      if (liveRegionTimerRef.current !== null) {
        window.clearTimeout(liveRegionTimerRef.current)
      }
    }
  }, [])

  return (
    <article aria-labelledby={headingId} data-status={operation.status} className="p-4">
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
        <div className="min-w-0 flex-1">
          <div
            id={headingId}
            className="flex items-center gap-1.5 text-sm font-semibold text-light-text dark:text-dark-text"
          >
            {isCompleted ? (
              <CheckCircleIcon className="h-4 w-4 shrink-0 text-accent-green" />
            ) : null}
            {isFailed ? <XCircleIcon className="h-4 w-4 shrink-0 text-accent-red" /> : null}
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

          {/* What, then where. One muted line replaces the two full paths. */}
          <div className="mt-1 flex min-w-0 items-baseline gap-1.5 font-mono text-xs text-light-text-muted dark:text-dark-text-muted">
            <span className="flex min-w-0 flex-1 items-baseline gap-1">
              <span className="truncate text-light-text-soft dark:text-dark-text-soft">
                {previewNames.join(', ')}
              </span>
              {hiddenItemCount > 0 ? (
                <button
                  type="button"
                  aria-expanded={itemsExpanded}
                  aria-controls={itemListId}
                  onClick={() => setItemsExpanded((expanded) => !expanded)}
                  className="flex shrink-0 items-center gap-0.5 rounded text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:underline dark:text-accent-blue"
                >
                  +{formatCount(hiddenItemCount)} more
                  {itemsExpanded ? (
                    <ChevronUpIcon className="h-3 w-3" />
                  ) : (
                    <ChevronDownIcon className="h-3 w-3" />
                  )}
                </button>
              ) : null}
            </span>
            {operation.destinationDir ? (
              <>
                <ArrowRightIcon className="h-3 w-3 shrink-0 self-center text-accent-blue-light dark:text-accent-blue" />
                <span className="min-w-0 shrink truncate" title={operation.destinationDir}>
                  {operation.destinationDir}
                </span>
              </>
            ) : null}
          </div>
        </div>
      </div>

      {itemsExpanded ? (
        <ul
          id={itemListId}
          className="mt-2 flex max-h-queue-items flex-col gap-0.5 overflow-y-auto rounded-tab bg-light-skeleton px-2.5 py-1.5 font-mono text-uxs text-light-text-muted scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:bg-dark-skeleton dark:text-dark-text-muted dark:scrollbar-thumb-dark-text-faint"
        >
          {listedNames.map((name, index) => {
            const done = index < displayed.completedItems
            const current = !isFinished && index === displayed.completedItems
            return (
              <li
                key={name}
                className={`flex items-center gap-1 truncate ${
                  done
                    ? 'text-light-text-faint dark:text-dark-text-faint'
                    : current
                      ? 'text-light-text dark:text-dark-text'
                      : ''
                }`}
              >
                {done ? <CheckIcon className="h-3 w-3 shrink-0 text-accent-green" /> : null}
                <span className="truncate">{name}</span>
              </li>
            )
          })}
          {unlistedCount > 0 ? (
            <li className="text-light-text-faint dark:text-dark-text-faint">
              +{formatCount(unlistedCount)} more
            </li>
          ) : null}
        </ul>
      ) : null}

      <span className="sr-only" aria-live="polite" aria-atomic="true">
        {liveAnnouncement}
      </span>

      {showProgress ? (
        <>
          <div className="mt-3 flex items-baseline justify-between gap-3">
            <span className="min-w-0 truncate text-sm tabular-nums text-light-text dark:text-dark-text">
              {summary}
            </span>
            <span
              className={`shrink-0 font-mono text-uxs tabular-nums ${
                isFailed ? 'text-accent-red' : 'text-light-text-muted dark:text-dark-text-muted'
              }`}
            >
              {roundedPercent}%
            </span>
          </div>
          <div
            role="progressbar"
            aria-labelledby={headingId}
            aria-valuemin={0}
            aria-valuemax={100}
            aria-valuenow={roundedPercent}
            className="mt-2 h-1.5 overflow-hidden rounded-full bg-light-skeleton dark:bg-dark-skeleton"
          >
            <span
              className={`block h-full rounded-full ${
                isPaused
                  ? 'bg-light-text-muted dark:bg-dark-text-muted'
                  : 'bg-accent-blue-light dark:bg-accent-blue'
              }`}
              style={{ width: `${percent}%` }}
            />
          </div>
        </>
      ) : null}

      {showCurrentFile ? (
        <>
          <div className="mt-2.5 flex min-w-0 items-baseline justify-between gap-3">
            <span
              className="min-w-0 truncate text-xs text-light-text-muted dark:text-dark-text-muted"
              title={displayedFileName ?? undefined}
            >
              {displayedFileName ?? ' '}
            </span>
            <span className="shrink-0 font-mono text-uxs tabular-nums text-light-text-muted dark:text-dark-text-muted">
              {itemIndex}
            </span>
          </div>
          {showRate ? (
            <div className="mt-1 font-mono text-uxs tabular-nums text-light-text-muted dark:text-dark-text-muted">
              {rate}
            </div>
          ) : null}
        </>
      ) : null}

      {isFailed && operation.errorMessage ? (
        <div className="mt-3 rounded-tab bg-accent-red-soft px-3 py-2 text-uxs text-accent-red">
          {operation.errorMessage}
        </div>
      ) : isCancelled ? (
        <div className="mt-3 rounded-tab border border-light-border px-3 py-2 text-uxs text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
          Job cancelled. Any completed file changes were kept.
        </div>
      ) : showChart ? (
        <div className="mt-2.5">
          <ThroughputChart
            samples={view.samples}
            currentPercent={Math.min(100, Math.max(0, displayed.progressPercent))}
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

        {!isFinished ? (
          <button
            type="button"
            onClick={onSkip}
            disabled={!isPaused}
            className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            <SkipForwardIcon className="h-3.5 w-3.5" /> Skip
          </button>
        ) : null}

        {!isFinished ? (
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
