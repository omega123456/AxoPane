import { useCallback, useEffect, useMemo } from 'react'
import { QueuePanel } from '@/components/queue/QueuePanel'
import { QueueToast } from '@/components/queue/QueueToast'
import { onQueueConflict, onQueueProgress, onQueueRemoved } from '@/lib/ipc/events'
import { queueSnapshot } from '@/lib/queue-commands'
import { orderedOperations, useQueueStore } from '@/stores/queue-store'

/**
 * Bottom-right transfer queue surface: a collapsed toast that expands into the
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

  useEffect(() => {
    void queueSnapshot().then(hydrate)
  }, [hydrate])

  useEffect(() => {
    const unlistenProgress = onQueueProgress((event) => applyProgress(event))
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

      const target = orderedOperations(useQueueStore.getState())[0]
      switch (event.key) {
        case 'Escape':
          event.preventDefault()
          setExpanded(false)
          break
        case ' ':
          if (target) {
            event.preventDefault()
            if (target.status === 'paused') {
              resume(target.operationId)
            } else if (target.status === 'active') {
              pause(target.operationId)
            }
          }
          break
        case 'Delete':
          if (target) {
            event.preventDefault()
            cancel(target.operationId)
          }
          break
        case 'r':
        case 'R':
          if (target && target.status === 'failed') {
            event.preventDefault()
            retry(target.operationId)
          }
          break
        default:
          break
      }
    },
    [cancel, conflict, expanded, pause, resume, retry, setExpanded],
  )

  useEffect(() => {
    window.addEventListener('keydown', onKeyDown)
    return () => window.removeEventListener('keydown', onKeyDown)
  }, [onKeyDown])

  if (operations.length === 0 && !conflict) {
    return null
  }

  return (
    <>
      <div className="pointer-events-none absolute inset-x-0 bottom-status z-20 flex justify-end p-3">
        <div className="pointer-events-auto">
          {expanded ? <QueuePanel /> : <QueueToast />}
        </div>
      </div>
    </>
  )
}
