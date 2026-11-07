import { useCallback, useEffect, useMemo, useRef } from 'react'
import { QueuePanel } from '@/components/queue/QueuePanel'
import { QueueToast } from '@/components/queue/QueueToast'
import { onQueueConflict, onQueueProgress, onQueueRemoved } from '@/lib/ipc/events'
import { queueSnapshot } from '@/lib/queue-commands'
import { openAdminRestartDialog } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { isTerminal, orderedOperations, useQueueStore } from '@/stores/queue-store'
import { caseInsensitiveLetterKeys, handleKeyByTable, type KeyHandlerTable } from '@/lib/key-table'
import type { OpProgress, OpStatus } from '@/lib/types/ipc'

type QueueKeyActions = {
  collapse: () => void
  pause: (operationId: string) => void
  resume: (operationId: string) => void
  cancel: (operationId: string) => void
  retry: (operationId: string) => void
}

function togglePauseHandler(target: OpProgress, actions: QueueKeyActions) {
  const togglePause: Partial<Record<OpStatus, () => void>> = {
    paused: () => actions.resume(target.operationId),
    active: () => actions.pause(target.operationId),
  }
  return togglePause[target.status] ?? (() => undefined)
}

function retryKeys(target: OpProgress, actions: QueueKeyActions): KeyHandlerTable {
  if (target.status !== 'failed') {
    return {}
  }
  return caseInsensitiveLetterKeys({ r: () => actions.retry(target.operationId) })
}

function queueKeyTable(target: OpProgress | undefined, actions: QueueKeyActions): KeyHandlerTable {
  const collapseKey: KeyHandlerTable = { Escape: actions.collapse }
  if (!target) {
    return collapseKey
  }
  return {
    ...collapseKey,
    ' ': togglePauseHandler(target, actions),
    Delete: () => actions.cancel(target.operationId),
    ...retryKeys(target, actions),
  }
}

/**
 * Bottom-right job queue surface: a collapsed toast that expands into the
 * TransferCard-faithful panel. Owns the
 * `queue://*` event wiring and the queue keyboard model.
 */
export function QueueOverlay() {
  const order = useQueueStore((state) => state.order)
  const operationsMap = useQueueStore((state) => state.operations)
  const conflictsMap = useQueueStore((state) => state.conflicts)
  const operations = useMemo(
    () =>
      order
        .map((id) => operationsMap[id])
        .filter((operation): operation is NonNullable<typeof operation> => operation !== undefined),
    [order, operationsMap],
  )
  const conflict = useMemo(() => {
    for (const id of order) {
      if (conflictsMap[id]) {
        return conflictsMap[id]
      }
    }
    return undefined
  }, [order, conflictsMap])
  const expanded = useQueueStore((state) => state.expanded)
  const setExpanded = useQueueStore((state) => state.setExpanded)
  const applyProgress = useQueueStore((state) => state.applyProgress)
  const applyConflict = useQueueStore((state) => state.applyConflict)
  const removeOperation = useQueueStore((state) => state.removeOperation)
  const hydrate = useQueueStore((state) => state.hydrate)
  const pause = useQueueStore((state) => state.pause)
  const resume = useQueueStore((state) => state.resume)
  const cancel = useQueueStore((state) => state.cancel)
  const retry = useQueueStore((state) => state.retry)
  const autoExpandActiveQueueToasts = useConfigStore((state) => state.autoExpandActiveQueueToasts)
  const hadActiveWorkRef = useRef(false)

  useEffect(() => {
    void queueSnapshot().then(hydrate)
  }, [hydrate])

  useEffect(() => {
    const unlistenProgress = onQueueProgress((event) => {
      applyProgress(event)
      // A job the OS refused for lack of permissions cannot be retried in
      // place. Offer the elevated restart instead of a red line in the card.
      if (event.status === 'failed' && event.errorMessage) {
        openAdminRestartDialog(event.errorMessage)
      }
    })
    const unlistenConflict = onQueueConflict((event) => applyConflict(event))
    const unlistenRemoved = onQueueRemoved((event) => removeOperation(event))
    return () => {
      void unlistenProgress.then((unlisten) => unlisten())
      void unlistenConflict.then((unlisten) => unlisten())
      void unlistenRemoved.then((unlisten) => unlisten())
    }
  }, [applyConflict, applyProgress, removeOperation])

  // Queue keyboard model. Active only while the panel is expanded and no
  // conflict modal is open (the modal owns its own keys).
  const onKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (conflict) {
        return
      }
      if (!expanded) {
        return
      }

      const target = orderedOperations(useQueueStore.getState())[0] as OpProgress | undefined
      const keyTable = queueKeyTable(target, {
        collapse: () => setExpanded(false),
        pause,
        resume,
        cancel,
        retry,
      })
      handleKeyByTable(event, keyTable)
    },
    [cancel, conflict, expanded, pause, resume, retry, setExpanded],
  )

  useEffect(() => {
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onKeyDown])

  const hasActiveWork = operations.some((operation) => !isTerminal(operation.status))

  useEffect(() => {
    if (autoExpandActiveQueueToasts && !expanded && hasActiveWork && !hadActiveWorkRef.current) {
      setExpanded(true)
    }
    hadActiveWorkRef.current = hasActiveWork
  }, [autoExpandActiveQueueToasts, expanded, hasActiveWork, setExpanded])

  if (operations.length === 0 && !conflict) {
    return null
  }

  return (
    <>
      <div className="pointer-events-none absolute inset-x-0 bottom-status z-20 flex justify-end p-3">
        <div className="pointer-events-auto">{expanded ? <QueuePanel /> : <QueueToast />}</div>
      </div>
    </>
  )
}
