const MACRO_CONTINUATION_WINDOW_MS = 100
// Webviews keep the OS click count running across up-navigations, so ".." pairs raw clicks instead of trusting dblclick.
const PARENT_CLICK_PAIR_WINDOW_MS = 500
const DOUBLE_CLICK_COUNT = 2

export type PointerActivationState = {
  lastActivationAt: number | null
  parentPendingClickAt: number | null
}

export type PointerActivationResult = {
  shouldActivate: boolean
  next: PointerActivationState
}

export const INITIAL_POINTER_ACTIVATION: PointerActivationState = {
  lastActivationAt: null,
  parentPendingClickAt: null,
}

function isMacroContinuation(lastActivationAt: number | null, eventTimeStamp: number) {
  if (lastActivationAt === null) {
    return false
  }
  return eventTimeStamp - lastActivationAt < MACRO_CONTINUATION_WINDOW_MS
}

function isStrayExtraClick(
  state: PointerActivationState,
  clickCount: number,
  eventTimeStamp: number,
) {
  const beyondDoubleClick = clickCount > DOUBLE_CLICK_COUNT
  return beyondDoubleClick && isMacroContinuation(state.lastActivationAt, eventTimeStamp)
}

function completesParentClickPair(pendingClickAt: number | null, eventTimeStamp: number) {
  if (pendingClickAt === null) {
    return false
  }
  return eventTimeStamp - pendingClickAt <= PARENT_CLICK_PAIR_WINDOW_MS
}

export function entryPointerActivation(
  state: PointerActivationState,
  clickCount?: number,
  eventTimeStamp?: number,
): PointerActivationResult {
  if (clickCount === undefined) {
    return { shouldActivate: true, next: state }
  }
  if (eventTimeStamp === undefined) {
    return { shouldActivate: true, next: state }
  }
  if (isStrayExtraClick(state, clickCount, eventTimeStamp)) {
    return { shouldActivate: false, next: state }
  }
  return { shouldActivate: true, next: { ...state, lastActivationAt: eventTimeStamp } }
}

export function parentPointerActivation(
  state: PointerActivationState,
  eventTimeStamp?: number,
): PointerActivationResult {
  if (eventTimeStamp === undefined) {
    return { shouldActivate: true, next: state }
  }
  if (isMacroContinuation(state.lastActivationAt, eventTimeStamp)) {
    return { shouldActivate: false, next: state }
  }
  if (!completesParentClickPair(state.parentPendingClickAt, eventTimeStamp)) {
    return { shouldActivate: false, next: { ...state, parentPendingClickAt: eventTimeStamp } }
  }
  return {
    shouldActivate: true,
    next: { lastActivationAt: eventTimeStamp, parentPendingClickAt: null },
  }
}
