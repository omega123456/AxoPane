/** Thin, transparent-track scrollbar used by every scrolling surface. */
export const SCROLLBAR_THIN =
  'scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary'

export const TEXT_SECONDARY = 'text-light-text-secondary dark:text-dark-text-secondary'

const DROP_TARGET_OUTLINE =
  'outline outline-2 -outline-offset-2 outline-light-accent dark:outline-dark-accent'

export function dropTargetOutline(isDropTarget: boolean) {
  return isDropTarget ? DROP_TARGET_OUTLINE : ''
}

export const NO_CLASS = ''

export const HOVER_FILL = 'bg-light-hover dark:bg-dark-hover'

export const HOVER_ON_POINTER = 'hover:bg-light-hover dark:hover:bg-dark-hover'

const DROP_TARGET_RING = 'ring-2 ring-inset ring-light-accent dark:ring-dark-accent'

const QUARTER_TURN = 'rotate-90'

const HALF_TURN = 'rotate-180'

const SPINNING = 'animate-spin'

export function dropTargetRing(isDropTarget: boolean) {
  return isDropTarget ? DROP_TARGET_RING : NO_CLASS
}

export function activeFillClass(isActive: boolean) {
  return isActive ? HOVER_FILL : NO_CLASS
}

export function menuItemStateClass(isActive: boolean) {
  return isActive ? HOVER_FILL : HOVER_ON_POINTER
}

export function disclosureChevronClass(isExpanded: boolean) {
  return isExpanded ? QUARTER_TURN : NO_CLASS
}

export function flippedChevronClass(isFlipped: boolean) {
  return isFlipped ? HALF_TURN : NO_CLASS
}

export function spinningClass(isSpinning: boolean) {
  return isSpinning ? SPINNING : NO_CLASS
}
