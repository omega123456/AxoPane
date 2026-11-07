/** Thin, transparent-track scrollbar used by every scrolling surface. */
export const SCROLLBAR_THIN =
  'scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary'

export const TEXT_SECONDARY = 'text-light-text-secondary dark:text-dark-text-secondary'

const DROP_TARGET_OUTLINE =
  'outline outline-2 -outline-offset-2 outline-light-accent dark:outline-dark-accent'

export function dropTargetOutline(isDropTarget: boolean) {
  return isDropTarget ? DROP_TARGET_OUTLINE : ''
}
