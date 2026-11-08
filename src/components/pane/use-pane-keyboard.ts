import { useEffect, useLayoutEffect, useRef, type KeyboardEvent } from 'react'
import { executeCommand } from '@/lib/commands'
import type { CommandId, Shortcut } from '@/lib/types/ipc'
import { resolveCommandForEvent } from '@/lib/keymap'
import { handleKeyByTable, type KeyHandlerTable } from '@/lib/key-table'
import { isEditableTarget } from '@/lib/editable-target'
import { focusFilterInput, registerPaneFilterKeys } from '@/lib/pane-filter-keys'
import type { PaneId } from '@/types/pane'
import { isTypeaheadKey } from './pane-keyboard'

const ENTER_KEY = 'Enter'
const SEARCH_EVERYWHERE_COMMAND: CommandId = 'searchEverywhere'

type PaneKeyboardOptions = {
  paneId: PaneId
  acceptsKeys: boolean
  keymap: Record<CommandId, Shortcut[]>
  focusedEntryId: string | null
  navigationKeyTable: () => KeyHandlerTable
  openFromFilter: (entryId: string) => void
}

type PaneKeyEvent = KeyboardEvent<HTMLElement>

function swallowedByEditor(event: PaneKeyEvent, fromFilter: boolean) {
  return isEditableTarget(event.target) && !fromFilter
}

export function usePaneKeyboard(options: PaneKeyboardOptions) {
  const { paneId, keymap } = options

  function shouldIgnore(event: PaneKeyEvent, fromFilter: boolean) {
    return !options.acceptsKeys || event.defaultPrevented || swallowedByEditor(event, fromFilter)
  }

  function handleFilterKey(event: PaneKeyEvent) {
    const entryToOpen = event.key === ENTER_KEY ? options.focusedEntryId : null
    if (entryToOpen) {
      event.preventDefault()
      options.openFromFilter(entryToOpen)
      return
    }
    if (resolveCommandForEvent(event.nativeEvent, keymap) === SEARCH_EVERYWHERE_COMMAND) {
      event.preventDefault()
      executeCommand(SEARCH_EVERYWHERE_COMMAND, paneId)
    }
  }

  function runBoundCommand(event: PaneKeyEvent) {
    const commandId = resolveCommandForEvent(event.nativeEvent, keymap)
    if (!commandId) {
      return
    }
    event.preventDefault()
    event.stopPropagation()
    executeCommand(commandId, paneId)
  }

  function handlePaneKeyDown(event: PaneKeyEvent, fromFilter: boolean) {
    if (shouldIgnore(event, fromFilter)) {
      return
    }
    if (handleKeyByTable(event, options.navigationKeyTable())) {
      return
    }
    if (fromFilter) {
      handleFilterKey(event)
      return
    }
    if (isTypeaheadKey(event)) {
      focusFilterInput()
      return
    }
    runBoundCommand(event)
  }

  const paneKeyDownRef = useRef(handlePaneKeyDown)
  useLayoutEffect(() => {
    paneKeyDownRef.current = handlePaneKeyDown
  })
  useEffect(
    () => registerPaneFilterKeys(paneId, (event) => paneKeyDownRef.current(event, true)),
    [paneId],
  )

  return handlePaneKeyDown
}
