import { useId } from 'react'
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
import { formatBytes, formatCount, formatEta, formatRate } from '@/lib/format'
import type { OpProgress } from '@/lib/types/ipc'

type JobCardProps = {
  operation: OpProgress
  hasConflict: boolean
  reorderable: boolean
  onPause: () => void
  onResume: () => void
  onCancel: () => void
  onSkip: () => void
  onRetry: () => void
  onResolve: () => void
  onMoveUp?: () => void
  onMoveDown?: () => void
}

const SPARK_BARS = [
  'h-2',
  'h-4',
  'h-2.5',
  'h-5',
  'h-6',
  'h-4',
  'h-5',
  'h-3',
  'h-6',
  'h-4',
  'h-7',
  'h-3.5',
  'h-5',
  'h-8',
  'h-4',
  'h-6',
]

function verb(operation: OpProgress) {
  return operation.kind === 'move' ? 'Moving' : 'Copying'
}

export function JobCard({
  operation,
  hasConflict,
  reorderable,
  onPause,
  onResume,
  onCancel,
  onSkip,
  onRetry,
  onResolve,
  onMoveUp,
  onMoveDown,
}: JobCardProps) {
  const headingId = useId()
  const percent = Math.min(100, Math.max(0, operation.progressPercent))
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
  const isConflict = operation.status === 'conflict' || hasConflict

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
              <span className="truncate">
                {isCompleted
                  ? `${verb(operation)} complete`
                  : isFailed
                    ? `${verb(operation)} failed`
                    : `${verb(operation)} ${formatCount(operation.totalItems)} items`}
              </span>
            </div>
            <div className="mt-1 truncate font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
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
          {Math.round(percent)}%
        </span>
      </div>

      <div className="mt-3.5 h-2 overflow-hidden rounded-full bg-light-skeleton dark:bg-dark-skeleton">
        <div
          className={`h-full rounded-full ${
            isFailed
              ? 'bg-accent-red'
              : 'bg-accent-blue-light dark:bg-accent-blue'
          }`}
          style={{ width: `${percent}%` }}
        />
      </div>

      <div className="mt-2 flex flex-wrap items-center gap-2 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        <span className="text-light-text-soft dark:text-dark-text-soft">
          {formatRate(operation.bytesPerSecond)}
        </span>
        <span className="text-light-text-faint dark:text-dark-text-faint">·</span>
        <span>
          {operation.etaSeconds !== null
            ? formatEta(operation.etaSeconds)
            : 'estimating…'}
        </span>
        <span className="text-light-text-faint dark:text-dark-text-faint">·</span>
        <span>
          {formatCount(operation.completedItems)} / {formatCount(operation.totalItems)} items
        </span>
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
          <div className="mt-2 h-1 overflow-hidden rounded-full bg-light-skeleton dark:bg-dark-skeleton">
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
      ) : (
        <div className="mt-3.5 flex h-queue-bars items-end gap-0.5">
          {SPARK_BARS.map((bar, index) => (
            <span
              key={`${bar}-${index}`}
              className={`flex-1 rounded-sm bg-accent-blue-light opacity-60 dark:bg-accent-blue ${bar} ${
                index % 4 === 1 ? 'opacity-70' : ''
              }`}
            />
          ))}
        </div>
      )}

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
          <button
            type="button"
            onClick={onRetry}
            className="flex items-center gap-1.5 rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            <RotateCcwIcon className="h-3.5 w-3.5" /> Retry
          </button>
        ) : isCompleted ? null : isPaused ? (
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

        {!isCompleted && !isFailed ? (
          <button
            type="button"
            onClick={onSkip}
            className="flex items-center gap-1.5 rounded-md border border-light-border px-3.5 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            <SkipForwardIcon className="h-3.5 w-3.5" /> Skip
          </button>
        ) : null}

        {!isCompleted ? (
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
