import type { KeyboardEvent } from 'react'
import type { PaneId } from '@/types/pane'

type FilterKeyHandler = (event: KeyboardEvent<HTMLInputElement>) => void

// The shared filter lives in the command bar, outside the panes, so its keys
// no longer bubble through a pane. Each pane registers how it handles keys
// typed into the filter (arrows, paging, Enter) and the filter forwards them
// to whichever pane is active.
const handlers = new Map<PaneId, FilterKeyHandler>()

export function registerPaneFilterKeys(paneId: PaneId, handler: FilterKeyHandler) {
  handlers.set(paneId, handler)
  return () => {
    if (handlers.get(paneId) === handler) {
      handlers.delete(paneId)
    }
  }
}

export function forwardPaneFilterKey(paneId: PaneId, event: KeyboardEvent<HTMLInputElement>) {
  handlers.get(paneId)?.(event)
}

/** The shared filter field, focused when the user types into a list. */
export function focusFilterInput() {
  const input = document.querySelector<HTMLInputElement>('input[data-filter-input]')
  input?.focus()
  input?.setSelectionRange(input.value.length, input.value.length)
}
