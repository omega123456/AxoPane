import { useRef } from 'react'
import { executeCommand } from '@/lib/commands'
import type { PaneId } from '@/types/pane'
import {
  entryPointerActivation,
  INITIAL_POINTER_ACTIVATION,
  parentPointerActivation,
  type PointerActivationResult,
} from './pointer-activation'
import { PARENT_ROW_ID } from './use-pane-focus'

export function usePointerActivation(paneId: PaneId) {
  const activationStateRef = useRef(INITIAL_POINTER_ACTIVATION)

  function activateEntry(entryId: string) {
    executeCommand('open', paneId, entryId)
  }

  function applyActivation(result: PointerActivationResult, entryId: string) {
    activationStateRef.current = result.next
    if (result.shouldActivate) activateEntry(entryId)
  }

  function activateEntryFromPointer(entryId: string, clickCount?: number, eventTimeStamp?: number) {
    const result = entryPointerActivation(activationStateRef.current, clickCount, eventTimeStamp)
    applyActivation(result, entryId)
  }

  function activateParentFromPointer(eventTimeStamp?: number) {
    const result = parentPointerActivation(activationStateRef.current, eventTimeStamp)
    applyActivation(result, PARENT_ROW_ID)
  }

  return { activateEntry, activateEntryFromPointer, activateParentFromPointer }
}
