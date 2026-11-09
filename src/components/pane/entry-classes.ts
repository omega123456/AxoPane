import { HOVER_ON_POINTER } from '@/lib/class-names'
import { EMPTY_VALUE, itemNoun } from '@/lib/format'
const FOCUS_CURSOR_RING = 'ring-1 ring-inset ring-light-accent dark:ring-dark-accent'
const ACTIVE_SELECTION_FILL = 'bg-light-tint dark:bg-dark-tint'
const INACTIVE_SELECTION_FILL = 'bg-light-neutral-selection dark:bg-dark-neutral-selection'

type EntryHighlight = {
  isActivePane: boolean
  isSelected: boolean
  isFocused: boolean
}

export function cutFadeClass(isCut: boolean) {
  return isCut ? 'opacity-50' : ''
}

function focusCursorClass(showCursor: boolean) {
  return showCursor ? FOCUS_CURSOR_RING : ''
}

function rowFillClass({ isActivePane, isSelected }: EntryHighlight) {
  if (!isSelected) {
    return HOVER_ON_POINTER
  }
  return isActivePane ? ACTIVE_SELECTION_FILL : INACTIVE_SELECTION_FILL
}

export function rowHighlightClass(highlight: EntryHighlight) {
  const showCursor = highlight.isFocused && highlight.isActivePane
  return `${rowFillClass(highlight)} ${focusCursorClass(showCursor)}`
}

export function rowDropTargetClass(isDropTarget: boolean) {
  return isDropTarget
    ? `ring-2 ring-inset ring-light-accent dark:ring-dark-accent ${ACTIVE_SELECTION_FILL}`
    : ''
}

function cardSelectionClass(isActivePane: boolean) {
  return isActivePane
    ? 'border-light-accent bg-light-tint dark:border-dark-accent dark:bg-dark-tint'
    : `border-transparent ${INACTIVE_SELECTION_FILL}`
}

export function cardHighlightClass(highlight: EntryHighlight & { isDropTarget: boolean }) {
  const filled = highlight.isSelected || highlight.isDropTarget
  const fill = filled
    ? cardSelectionClass(highlight.isActivePane)
    : `border-transparent ${HOVER_ON_POINTER}`
  const showCursor = highlight.isFocused && highlight.isActivePane && !highlight.isSelected
  return `rounded-overlay border ${fill} ${focusCursorClass(showCursor)}`
}

export function itemCountLabel(itemCount: number | null) {
  if (itemCount === null) {
    return EMPTY_VALUE
  }
  return `${itemCount} ${itemNoun(itemCount)}`
}
