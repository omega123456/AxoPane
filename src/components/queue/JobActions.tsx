import {
  GripVerticalIcon,
  PauseIcon,
  PlayIcon,
  RotateCcwIcon,
  SkipForwardIcon,
} from '@/components/icons'
import { Button } from '@/components/controls'
import type { JobFlags } from '@/lib/queue-format'

export type JobActionHandlers = {
  onPause: () => void
  onResume: () => void
  onCancel: () => void
  onDismiss: () => void
  onSkip: () => void
  onRetry: () => void
  onResolve: () => void
}

function PrimaryJobActions({ flags, handlers }: { flags: JobFlags; handlers: JobActionHandlers }) {
  if (flags.isConflict) {
    return (
      <Button variant="primary" onClick={handlers.onResolve}>
        Resolve conflict
      </Button>
    )
  }
  if (flags.isFailed) {
    return (
      <>
        <Button onClick={handlers.onRetry}>
          <RotateCcwIcon className="size-4" /> Retry
        </Button>
        <Button variant="subtle" onClick={handlers.onDismiss}>
          Dismiss
        </Button>
      </>
    )
  }
  if (flags.isFinished) {
    return <Button onClick={handlers.onDismiss}>Dismiss</Button>
  }
  if (flags.isPaused) {
    return (
      <Button onClick={handlers.onResume}>
        <PlayIcon className="size-4" /> Resume
      </Button>
    )
  }
  if (flags.isPending) {
    return null
  }
  return (
    <Button onClick={handlers.onPause}>
      <PauseIcon className="size-4" /> Pause
    </Button>
  )
}

export function JobActions({
  flags,
  handlers,
  onMoveDown,
}: {
  flags: JobFlags
  handlers: JobActionHandlers
  onMoveDown: (() => void) | undefined
}) {
  return (
    <div className="mt-1 flex flex-wrap items-center gap-2">
      <PrimaryJobActions flags={flags} handlers={handlers} />

      {flags.isUnderway ? (
        <Button onClick={handlers.onSkip} disabled={!flags.isPaused}>
          <SkipForwardIcon className="size-4" /> Skip
        </Button>
      ) : null}

      {!flags.isFinished ? (
        <Button variant="subtle" onClick={handlers.onCancel}>
          Cancel
        </Button>
      ) : null}

      {onMoveDown ? (
        <Button
          variant="subtle"
          aria-label="Move job down"
          onClick={onMoveDown}
          className="ml-auto"
        >
          Move down
        </Button>
      ) : null}
    </div>
  )
}

export function MoveUpHandle({ onMoveUp }: { onMoveUp: (() => void) | undefined }) {
  return (
    <Button
      variant="subtle"
      aria-label="Move job up"
      onClick={onMoveUp}
      disabled={!onMoveUp}
      size="compact"
      className="-ml-2"
    >
      <GripVerticalIcon className="size-3.5" />
    </Button>
  )
}
