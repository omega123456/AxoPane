import type { KeyboardEvent } from 'react'
import type { PaneId } from '@/types/pane'

type FilterKeyHandler = (event: KeyboardEvent<HTMLInputElement>) => void

const handlers = new Map<PaneId, FilterKeyHandler>()

export function registerPaneFilterKeys(paneId: PaneId, handler: FilterKeyHandler) {
  handlers.set(paneId, handler)
  return () => {
    const stillRegistered = handlers.get(paneId) === handler
    if (stillRegistered) {
      handlers.delete(paneId)
    }
  }
}

export function forwardPaneFilterKey(paneId: PaneId, event: KeyboardEvent<HTMLInputElement>) {
  handlers.get(paneId)?.(event)
}

export function focusFilterInput() {
  const input = document.querySelector<HTMLInputElement>('input[data-filter-input]')
  input?.focus()
  input?.setSelectionRange(input.value.length, input.value.length)
}
