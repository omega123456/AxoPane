import { onDragPosition } from '@/lib/ipc/events'
import { log } from '@/lib/app-log-commands'
import { useDragCursorStore, type DragCursor } from '@/stores/drag-cursor-store'
import type { DropKind } from '@/lib/drag-drop'
import type { DragPositionEvent } from '@/lib/types/ipc'
import { BOTH_SIDES } from '@/lib/numbers'

function setCursor(cursor: DragCursor | null) {
  useDragCursorStore.getState().setCursor(cursor)
}

type DragModifiers = {
  ctrlKey: boolean
  shiftKey: boolean
  altKey: boolean
  metaKey: boolean
}

type DragSession = {
  modifiers: DragModifiers
  target: Element | null
  x: number
  y: number
  regionChanges: number
}

let session: DragSession | null = null

function syntheticDataTransfer() {
  return {
    dropEffect: 'none',
    effectAllowed: 'copyMove',
    setData: () => {},
    getData: () => '',
  }
}

type DragDispatch = {
  type: string
  target: Element
  relatedTarget?: Element | null
}

function dispatchDragEvent(
  { type, target, relatedTarget = null }: DragDispatch,
  session: DragSession,
) {
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: session.x,
    clientY: session.y,
    relatedTarget,
    ...session.modifiers,
  })
  const transfer = syntheticDataTransfer()
  Object.defineProperty(event, 'dataTransfer', { value: transfer })
  target.dispatchEvent(event)
  return { accepted: event.defaultPrevented, dropEffect: transfer.dropEffect }
}

function toViewport({ cursorX, cursorY, frameWidth, frameHeight }: DragPositionEvent) {
  const ratio = window.devicePixelRatio || 1
  const sideChrome = Math.max(0, (frameWidth / ratio - window.innerWidth) / BOTH_SIDES)
  const topChrome = Math.max(0, frameHeight / ratio - window.innerHeight - sideChrome)
  return { x: cursorX / ratio - sideChrome, y: cursorY / ratio - topChrome }
}

function handlePosition(event: DragPositionEvent) {
  if (!session) {
    return
  }
  const { x, y } = toViewport(event)
  session.modifiers = {
    ctrlKey: event.ctrlKey,
    shiftKey: event.shiftKey,
    altKey: event.altKey,
    metaKey: event.metaKey,
  }
  session.x = x
  session.y = y
  const target = document.elementFromPoint(x, y)
  const previous = session.target
  session.target = target
  if (!sameRegion(previous, target)) {
    session.regionChanges += 1
    if (previous) {
      dispatchDragEvent({ type: 'dragleave', target: previous, relatedTarget: target }, session)
    }
    if (target) {
      dispatchDragEvent({ type: 'dragenter', target, relatedTarget: previous }, session)
    }
  }
  if (!target) {
    setCursor(null)
    return
  }
  const { accepted, dropEffect } = dispatchDragEvent({ type: 'dragover', target }, session)
  const showsDropKind = accepted && isDropKind(dropEffect)
  setCursor(showsDropKind ? { x, y, kind: dropEffect } : null)
}

function isDropKind(effect: string): effect is DropKind {
  return effect === 'copy' || effect === 'move'
}

function sameRegion(previous: Element | null, next: Element | null) {
  const previousRow = entryRowId(previous)
  const nextRow = entryRowId(next)
  const eitherIsRow = previousRow !== null || nextRow !== null
  if (eitherIsRow) {
    return previousRow === nextRow
  }
  return sameOrNested(previous, next)
}

function entryRowId(element: Element | null) {
  return element?.closest('[data-entry-id]')?.getAttribute('data-entry-id') ?? null
}

function sameOrNested(previous: Element | null, next: Element | null) {
  if (previous === null) {
    return next === null
  }
  if (next === null) {
    return false
  }
  return previous === next || previous.contains(next) || next.contains(previous)
}

export function subscribeNativeDragPositions() {
  return onDragPosition((position) => {
    handlePosition(position)
  })
}

export function beginNativeDragBridge() {
  session = {
    modifiers: { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false },
    target: null,
    x: 0,
    y: 0,
    regionChanges: 0,
  }
}

export function endNativeDragBridge() {
  const finished = session
  session = null
  setCursor(null)
  if (!finished) {
    return
  }
  log.debug('native drag drop', {
    row: finished.target?.closest('[data-entry-id]')?.getAttribute('data-entry-id') ?? null,
    regionChanges: finished.regionChanges,
  })
  if (finished.target) {
    dispatchDragEvent({ type: 'drop', target: finished.target }, finished)
  }
}
