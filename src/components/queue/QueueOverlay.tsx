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
import { useOrderedOperations } from './use-ordered-operations'

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

function useFirstConflict() {
  const order = useQueueStore((state) => state.order)
  const conflictsMap = useQueueStore((state) => state.conflicts)
  return useMemo(() => {
    for (const id of order) {
      if (conflictsMap[id]) {
        return conflictsMap[id]
      }
    }
    return undefined
  }, [order, conflictsMap])
}

function useQueueEventSubscriptions() {
  const applyProgress = useQueueStore((state) => state.applyProgress)
  const applyConflict = useQueueStore((state) => state.applyConflict)
  const removeOperation = useQueueStore((state) => state.removeOperation)
  const hydrate = useQueueStore((state) => state.hydrate)

  useEffect(() => {
    void queueSnapshot().then(hydrate)
  }, [hydrate])

  useEffect(() => {
    const unlistenProgress = onQueueProgress((event) => {
      applyProgress(event)
      // A job the OS refused for lack of permissions cannot be retried in
      // place. Offer the elevated restart instead of a red line in the card.
      if (event.status !== 'failed') return
      if (event.errorMessage) {
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
}

// Queue keyboard model. Active only while the panel is expanded and no
// conflict modal is open (the modal owns its own keys).
function useQueueKeyboard(hasConflict: boolean, expanded: boolean) {
  const setExpanded = useQueueStore((state) => state.setExpanded)
  const pause = useQueueStore((state) => state.pause)
  const resume = useQueueStore((state) => state.resume)
  const cancel = useQueueStore((state) => state.cancel)
  const retry = useQueueStore((state) => state.retry)

  const onKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (hasConflict) {
        return
      }
      if (!expanded) {
        return
      }

      const target = orderedOperations(useQueueStore.getState())[0] as OpProgress | undefined
      const collapse = () => setExpanded(false)
      handleKeyByTable(event, queueKeyTable(target, { collapse, pause, resume, cancel, retry }))
    },
    [cancel, hasConflict, expanded, pause, resume, retry, setExpanded],
  )

  useEffect(() => {
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onKeyDown])
}

function useAutoExpandOnActiveWork(operations: OpProgress[], expanded: boolean) {
  const setExpanded = useQueueStore((state) => state.setExpanded)
  const autoExpandActiveQueueToasts = useConfigStore((state) => state.autoExpandActiveQueueToasts)
  const hadActiveWorkRef = useRef(false)
  const hasActiveWork = operations.some((operation) => !isTerminal(operation.status))

  useEffect(() => {
    const workJustStarted = hasActiveWork && !hadActiveWorkRef.current
    const shouldAutoExpand = autoExpandActiveQueueToasts && !expanded && workJustStarted
    if (shouldAutoExpand) {
      setExpanded(true)
    }
    hadActiveWorkRef.current = hasActiveWork
  }, [autoExpandActiveQueueToasts, expanded, hasActiveWork, setExpanded])
}

/**
 * Bottom-right job queue surface: a collapsed toast that expands into the
 * TransferCard-faithful panel. Owns the
 * `queue://*` event wiring and the queue keyboard model.
 */
export function QueueOverlay() {
  const conflict = useFirstConflict()
  const operations = useOrderedOperations()
  const expanded = useQueueStore((state) => state.expanded)
  useQueueEventSubscriptions()
  useQueueKeyboard(Boolean(conflict), expanded)
  useAutoExpandOnActiveWork(operations, expanded)

  const isIdle = operations.length === 0 && !conflict
  if (isIdle) {
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
