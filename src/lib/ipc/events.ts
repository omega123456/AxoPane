import type { IpcEventMap } from '@/lib/types/ipc'
import { subscribeToEvent } from './client'

export function onDirPatch(handler: (payload: IpcEventMap['dir://patch']) => void) {
  return subscribeToEvent('dir://patch', handler)
}

export function onListChunk(handler: (payload: IpcEventMap['dir://list-chunk']) => void) {
  return subscribeToEvent('dir://list-chunk', handler)
}

export function onSizeState(handler: (payload: IpcEventMap['size://state']) => void) {
  return subscribeToEvent('size://state', handler)
}

export function onIconState(handler: (payload: IpcEventMap['icon://state']) => void) {
  return subscribeToEvent('icon://state', handler)
}

export function onVolumesChanged(handler: (payload: IpcEventMap['volumes://changed']) => void) {
  return subscribeToEvent('volumes://changed', handler)
}

export function onQueueProgress(handler: (payload: IpcEventMap['queue://progress']) => void) {
  return subscribeToEvent('queue://progress', handler)
}

export function onQueueConflict(handler: (payload: IpcEventMap['queue://conflict']) => void) {
  return subscribeToEvent('queue://conflict', handler)
}

export function onQueueRemoved(handler: (payload: IpcEventMap['queue://removed']) => void) {
  return subscribeToEvent('queue://removed', handler)
}

export function onWatchError(handler: (payload: IpcEventMap['watch://error']) => void) {
  return subscribeToEvent('watch://error', handler)
}
