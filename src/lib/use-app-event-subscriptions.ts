import { useEffect } from 'react'
import { installCloseGuard } from '@/lib/close-guard'
import {
  onDirSessionPatch,
  onIconState,
  onItemCountState,
  onSizeState,
  onThumbnailState,
  onVolumesChanged,
} from '@/lib/ipc/events'
import { subscribeNativeDragPositions } from '@/lib/native-drag-bridge'
import { createRafBatcher } from '@/lib/raf-batcher'
import {
  handleGlobalKeyDown,
  handleHistoryMouseUp,
  type GlobalKeyContext,
} from '@/lib/app-bootstrap'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useThumbnailStore } from '@/stores/thumbnail-store'
import { useUpdaterStore } from '@/stores/updater-store'
import { useModalOpen } from '@/lib/use-modal-open'
import type { ItemCountEvent } from '@/lib/types/ipc'

type Unlisten = () => void
type EventBatchSubscription<Event> = (onEvents: (events: Event[]) => void) => Promise<Unlisten>

function releaseWhenReady(unlistenPromise: Promise<Unlisten>) {
  void unlistenPromise.then((unlisten) => unlisten())
}

function subscribeImmediate(unlistenPromise: Promise<Unlisten>): Unlisten {
  return () => releaseWhenReady(unlistenPromise)
}

function subscribeBatched<Event>(
  subscribe: EventBatchSubscription<Event>,
  apply: (batch: Event[]) => void,
): Unlisten {
  const batcher = createRafBatcher<Event>(apply)
  const unlistenPromise = subscribe((events) => {
    for (const event of events) {
      batcher.push(event)
    }
  })
  return () => {
    releaseWhenReady(unlistenPromise)
    batcher.flush()
    batcher.cancel()
  }
}

const subscribeItemCounts: EventBatchSubscription<ItemCountEvent> = (onEvents) =>
  onItemCountState((event) => onEvents([event]))

function subscribeStoreEvents(): Unlisten {
  const panes = () => usePanesStore.getState()
  const unlisteners = [
    subscribeImmediate(onVolumesChanged((event) => panes().setVolumes(event.volumes))),
    subscribeBatched(onSizeState, (batch) => panes().applySizeStates(batch)),
    subscribeBatched(onIconState, (batch) => panes().applyIconStates(batch)),
    subscribeBatched(subscribeItemCounts, (batch) => panes().applyItemCountEvents(batch)),
    subscribeImmediate(onDirSessionPatch((event) => panes().applySessionPatch(event))),
    subscribeBatched(onThumbnailState, (batch) =>
      useThumbnailStore.getState().applyThumbnailResults(batch),
    ),
    subscribeImmediate(subscribeNativeDragPositions()),
  ]
  return () => {
    for (const unlisten of unlisteners) {
      unlisten()
    }
  }
}

function startCloseGuard(): Unlisten {
  let unlisten: Unlisten | undefined
  void installCloseGuard().then((dispose) => {
    unlisten = dispose
  })
  return () => unlisten?.()
}

function syncClipboardFromOs() {
  void useClipboardStore.getState().syncFromOs()
}

function listenToWindow<Name extends keyof WindowEventMap>(
  name: Name,
  listener: (event: WindowEventMap[Name]) => void,
): Unlisten {
  window.addEventListener(name, listener)
  return () => window.removeEventListener(name, listener)
}

function startPeriodicUpdateCheck(): Unlisten {
  useUpdaterStore.getState().startPeriodicCheck()
  return () => useUpdaterStore.getState().stopPeriodicCheck()
}

export function useAppEventSubscriptions() {
  const updateCheckInterval = useConfigStore((state) => state.updateCheckInterval)
  const defaultPaneMode = useLayoutStore((state) => state.defaultPaneMode)
  const keymap = useKeymapStore((state) => state.bindings)
  const modalOpen = useModalOpen()

  useEffect(() => startPeriodicUpdateCheck(), [updateCheckInterval])

  useEffect(() => startCloseGuard(), [])

  useEffect(() => {
    const context: GlobalKeyContext = { modalOpen, defaultPaneMode, keymap }
    return listenToWindow('keydown', (event) => handleGlobalKeyDown(event, context))
  }, [defaultPaneMode, keymap, modalOpen])

  useEffect(
    () => listenToWindow('mouseup', (event) => handleHistoryMouseUp(event, modalOpen)),
    [modalOpen],
  )

  useEffect(() => {
    syncClipboardFromOs()
    return listenToWindow('focus', syncClipboardFromOs)
  }, [])

  useEffect(() => subscribeStoreEvents(), [])
}
