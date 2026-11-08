import { useRef } from 'react'
import { log } from '@/lib/app-log-commands'
import { errorMessage } from '@/lib/error-message'
import { renameEntryInPane } from '@/lib/file-actions'
import { useInlineRenameStore, type InlineRenameState } from '@/stores/inline-rename-store'
import type { PaneId } from '@/types/pane'

function isRenameSubmittable(
  rename: InlineRenameState | null,
  alreadySubmitting: boolean,
): rename is InlineRenameState {
  return rename !== null && !rename.busy && !alreadySubmitting
}

function renameInPane(rename: InlineRenameState | null, paneId: PaneId) {
  if (rename?.paneId !== paneId) {
    return null
  }
  return rename
}

export function useInlineRenameHandlers(paneId: PaneId, focusPaneShell: () => void) {
  const rename = useInlineRenameStore((state) => renameInPane(state.rename, paneId))
  const { cancelRename, setBusy, setError, setValue } = useInlineRenameStore.getState()
  const renameSubmittingRef = useRef(false)
  const ignoreNextRenameBlurRef = useRef(false)

  function finishRename() {
    renameSubmittingRef.current = false
    cancelRename()
    focusPaneShell()
  }

  async function commitRename(activeRename: InlineRenameState, newName: string) {
    renameSubmittingRef.current = true
    setBusy(true)
    setError(null)
    try {
      await renameEntryInPane(activeRename.paneId, activeRename.path, newName)
      finishRename()
    } catch (cause) {
      const message = errorMessage(cause)
      log.error('inline rename failed', { paneId, path: activeRename.path, error: message })
      setError(message)
      setBusy(false)
      renameSubmittingRef.current = false
    }
  }

  async function submitRename() {
    if (!isRenameSubmittable(rename, renameSubmittingRef.current)) return
    const newName = rename.value.trim()
    if (!newName) {
      cancelRename()
      renameSubmittingRef.current = false
      return
    }
    if (newName === rename.initialValue) {
      finishRename()
      return
    }
    await commitRename(rename, newName)
  }

  function renameCancel() {
    ignoreNextRenameBlurRef.current = true
    finishRename()
  }

  function renameBlur() {
    if (ignoreNextRenameBlurRef.current) {
      ignoreNextRenameBlurRef.current = false
      return
    }
    void submitRename()
  }

  return { rename, setRenameValue: setValue, submitRename, renameCancel, renameBlur }
}
