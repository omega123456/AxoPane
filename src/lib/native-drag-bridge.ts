import { onDragPosition } from '@/lib/ipc/events'
import { log } from '@/lib/app-log-commands'
import { useDragCursorStore, type DragCursor } from '@/stores/drag-cursor-store'
import type { DropKind } from '@/lib/drag-drop'
import type { DragPositionEvent } from '@/lib/types/ipc'

function setCursor(cursor: DragCursor | null) {
  useDragCursorStore.getState().setCursor(cursor)
}

/**
 * Replays an OS drag session into the DOM.
 *
 * Once `start_native_drag` hands the gesture to the OS, the webview stops seeing
 * it: WKWebView does not deliver a self-originated drag session back to the page,
 * so no `dragenter`/`dragover`/`drop` ever fires and every in-app drop target goes
 * dark. Rust streams the cursor position instead (`drag://position`), and this
 * module turns that stream back into the DOM events the drop targets already
 * listen for — so `FilePane`, `TreeNode`, and friends need no drag-specific
 * knowledge of how the gesture is driven.
 */

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

/**
 * Drop handlers read their payload from `useDragStore` and only touch
 * `dataTransfer` to set `dropEffect`, so a minimal stand-in is enough — and it
 * keeps the synthesized events identical under jsdom, which implements neither
 * `DragEvent` nor `DataTransfer`.
 */
function syntheticDataTransfer() {
  return {
    dropEffect: 'none',
    effectAllowed: 'copyMove',
    setData: () => {},
    getData: () => '',
  }
}

function dispatchDragEvent(
  type: string,
  target: Element,
  session: DragSession,
  relatedTarget: Element | null = null,
) {
  // A `MouseEvent` named `dragover`/`drop`/… is enough: React resolves its
  // synthetic drag events by event type and reads `dataTransfer` off the native
  // event, so handlers cannot tell this from a webview-generated drag.
  const event = new MouseEvent(type, {
    bubbles: true,
    cancelable: true,
    clientX: session.x,
    clientY: session.y,
    // Real enter/leave pairs name the element on the other side of the boundary,
    // and drop handlers use it to ignore moves that never left their subtree.
    relatedTarget,
    ...session.modifiers,
  })
  const transfer = syntheticDataTransfer()
  Object.defineProperty(event, 'dataTransfer', { value: transfer })
  target.dispatchEvent(event)
  return { accepted: event.defaultPrevented, dropEffect: transfer.dropEffect }
}

/**
 * Converts an OS cursor position into viewport coordinates for `elementFromPoint`.
 *
 * Everything is measured, nothing assumed. `devicePixelRatio` converts physical
 * pixels to CSS pixels — and because it already folds in any webview page zoom,
 * the result stays correct at any zoom level. The window chrome (title bar, and
 * side borders where a platform has them) is whatever the frame has left over
 * once the viewport is accounted for, in those same CSS pixels.
 *
 * The app's own zoom control sets CSS `zoom` on the root element, which scales
 * rendered content but not the client coordinate space — `elementFromPoint`,
 * `clientX`, and `innerWidth` all stay in unzoomed viewport pixels, so it needs
 * no correction here. (`DetailsView` divides by that zoom only because it maps
 * client points into *content* space, which this never does.)
 */
function toViewport({ cursorX, cursorY, frameWidth, frameHeight }: DragPositionEvent) {
  const ratio = window.devicePixelRatio || 1
  const sideChrome = Math.max(0, (frameWidth / ratio - window.innerWidth) / 2)
  const topChrome = Math.max(0, frameHeight / ratio - window.innerHeight - sideChrome)
  return { x: cursorX / ratio - sideChrome, y: cursorY / ratio - topChrome }
}

function handlePosition(event: DragPositionEvent) {
  if (!session) {
    return
  }
  const { x, y } = toViewport(event)
  // Live, not sampled at drag start: the OS holds the keyboard for the whole
  // gesture, so this stream is the only way a modifier pressed mid-drag can
  // reach `resolveDropKind` — which reads it from the event at drop time.
  session.modifiers = {
    ctrlKey: event.ctrlKey,
    shiftKey: event.shiftKey,
    altKey: event.altKey,
    metaKey: event.metaKey,
  }
  session.x = x
  session.y = y
  // Outside the window this misses and returns null, which is exactly right: the
  // drop belongs to whichever application is under the pointer, not to us.
  const target = document.elementFromPoint(x, y)
  const previous = session.target
  session.target = target
  if (!sameRegion(previous, target)) {
    session.regionChanges += 1
    if (previous) {
      dispatchDragEvent('dragleave', previous, session, target)
    }
    if (target) {
      dispatchDragEvent('dragenter', target, session, previous)
    }
  }
  if (!target) {
    // Pointer is outside the window: the drop belongs to another application, and
    // its own cursor feedback takes over.
    setCursor(null)
    return
  }
  // A drop target marks itself valid by calling `preventDefault` and setting
  // `dropEffect`, exactly as it would for a webview drag — so the badge reports
  // what the existing handlers decided, with no second copy of that logic.
  const { accepted, dropEffect } = dispatchDragEvent('dragover', target, session)
  setCursor(accepted && isDropKind(dropEffect) ? { x, y, kind: dropEffect } : null)
}

function isDropKind(effect: string): effect is DropKind {
  return effect === 'copy' || effect === 'move'
}

/**
 * Whether two hit-test results belong to the same drop target.
 *
 * Element identity is not usable on its own: the pointer jitters between a row
 * and its own child text span, and a re-render can swap the row's DOM node
 * outright — leaving the previous element detached, so `contains` misses. Either
 * one fires a leave/enter pair every frame, which clears and re-sets the drop
 * highlight ~60 times a second and leaves the drop on whichever node happened to
 * win the last tick. Rows carry a stable `data-entry-id`, so compare that when
 * both sides are rows, and fall back to containment elsewhere (pane backgrounds
 * and tree nodes are stable nodes).
 */
function sameRegion(a: Element | null, b: Element | null) {
  const rowA = a?.closest('[data-entry-id]')?.getAttribute('data-entry-id') ?? null
  const rowB = b?.closest('[data-entry-id]')?.getAttribute('data-entry-id') ?? null
  if (rowA !== null || rowB !== null) {
    return rowA === rowB
  }
  if (a === null || b === null) {
    return a === b
  }
  return a === b || a.contains(b) || b.contains(a)
}

/**
 * Subscribes to the position stream for the lifetime of the app. Wired from
 * `App` alongside the other event subscriptions rather than per drag, because a
 * `listen()` started at drag time would race the first positions — and a drag
 * whose positions all arrive before the listener is attached would have no
 * target left to drop on.
 */
export function subscribeNativeDragPositions() {
  // Wrapped rather than passed by reference: subscribers are held in a Set keyed
  // by function identity, so two overlapping subscriptions of the same reference
  // collapse into one — and the first one's teardown then unsubscribes the
  // second. React's StrictMode double-invoke does exactly that.
  return onDragPosition((position) => {
    handlePosition(position)
  })
}

/**
 * Arms the bridge for a drag that is about to start. Modifiers arrive with the
 * position stream, so none are needed here.
 */
export function beginNativeDragBridge() {
  session = {
    modifiers: { ctrlKey: false, shiftKey: false, altKey: false, metaKey: false },
    target: null,
    x: 0,
    y: 0,
    regionChanges: 0,
  }
}

/**
 * Ends the drag, dropping on whatever the pointer last rested on. Called when the
 * OS session resolves, which is the only signal that the gesture is over.
 */
export function endNativeDragBridge() {
  const finished = session
  session = null
  setCursor(null)
  if (!finished) {
    return
  }
  // One line per gesture: where the drop landed and how often the target
  // changed. A region-change count near the position count means the hit-test
  // is churning rather than tracking.
  log.debug('native drag drop', {
    row: finished.target?.closest('[data-entry-id]')?.getAttribute('data-entry-id') ?? null,
    regionChanges: finished.regionChanges,
  })
  if (finished.target) {
    dispatchDragEvent('drop', finished.target, finished)
  }
}
