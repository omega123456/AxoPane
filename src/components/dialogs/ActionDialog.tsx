import { useState } from 'react'
import { Button, TextField } from '@/components/controls'
import {
  DialogError,
  DialogField,
  DialogList,
  DialogShell,
  DialogText,
} from '@/components/dialogs/DialogShell'
import { log } from '@/lib/app-log-commands'
import { errorMessage } from '@/lib/error-message'
import { createFileInPane, createFolderInPane } from '@/lib/file-actions'
import { restartAsAdmin } from '@/lib/ipc/commands'
import {
  useActionDialogStore,
  type ActionDialog as ActionDialogState,
} from '@/stores/action-dialog-store'
import { usePanesStore } from '@/stores/panes-store'
import { handleKeyByTable } from '@/lib/key-table'
import { ConfirmDialogShell } from './action-dialog/ConfirmDialogShell'
import {
  DeleteDialog,
  DeleteFromTrashDialog,
  EmptyTrashDialog,
} from './action-dialog/DeleteDialogs'
import { ArchiveConfirmDialog, TransferConfirmDialog } from './action-dialog/TransferDialogs'
import { useFocusOnMount, useSelectWhen } from '@/lib/use-focus-on-mount'
import { useActionDialogSubmit } from './action-dialog/use-action-dialog-submit'

export function ActionDialog() {
  const dialog = useActionDialogStore((state) => state.dialog)
  if (!dialog) {
    return null
  }

  if (dialog.kind === 'delete') {
    return <DeleteDialog dialog={dialog} />
  }

  if (dialog.kind === 'emptyTrash') {
    return <EmptyTrashDialog dialog={dialog} />
  }

  if (dialog.kind === 'calculateAllSizes') {
    return <CalculateAllSizesDialog dialog={dialog} />
  }

  if (dialog.kind === 'deleteFromTrash') {
    return <DeleteFromTrashDialog dialog={dialog} />
  }

  if (dialog.kind === 'adminRequired') {
    return <AdminRequiredDialog dialog={dialog} />
  }

  if (dialog.kind === 'transferConfirm') {
    return <TransferConfirmDialog dialog={dialog} />
  }

  if (dialog.kind === 'archiveConfirm') {
    return <ArchiveConfirmDialog dialog={dialog} />
  }

  return <PromptDialog dialog={dialog} />
}

const promptCopy = {
  newFolder: { title: 'New folder', label: 'Folder name', confirm: 'Create' },
  newFile: { title: 'New file', label: 'File name', confirm: 'Create' },
} as const

type PromptDialogState = Extract<ActionDialogState, { kind: 'newFolder' | 'newFile' }>

function createInPane(dialog: PromptDialogState, name: string) {
  if (dialog.kind === 'newFolder') {
    return createFolderInPane(dialog.paneId, name)
  }
  return createFileInPane(dialog.paneId, name)
}

function PromptDialog({ dialog }: { dialog: PromptDialogState }) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const [value, setValue] = useState('')
  const inputRef = useSelectWhen(true)
  const copy = promptCopy[dialog.kind]

  function submit() {
    const trimmed = value.trim()
    if (!trimmed || busy) {
      return
    }
    const failure = { message: 'file action failed', context: { kind: dialog.kind } }
    void run(() => createInPane(dialog, trimmed), failure)
  }

  function onKeyDown(event: React.KeyboardEvent) {
    handleKeyByTable(event, { Enter: submit, Escape: close })
  }

  return (
    <DialogShell
      label={copy.title}
      title={copy.title}
      onDismiss={close}
      footer={
        <>
          <Button variant="primary" disabled={busy || value.trim().length === 0} onClick={submit}>
            {copy.confirm}
          </Button>
          <Button onClick={close}>Cancel</Button>
        </>
      }
    >
      <DialogField label={copy.label} htmlFor="action-dialog-name">
        <TextField
          id="action-dialog-name"
          ref={inputRef}
          value={value}
          onChange={(event) => setValue(event.target.value)}
          onKeyDown={onKeyDown}
          aria-label={copy.label}
        />
      </DialogField>
      <DialogError message={error} />
    </DialogShell>
  )
}

/**
 * Windows refuses writes outside the user's own folders to a process that is
 * not elevated. The operation cannot be retried in place, so offer the only
 * fix: relaunch the application through the UAC prompt.
 */
function AdminRequiredDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'adminRequired' }>
}) {
  const close = useActionDialogStore((state) => state.close)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const confirmRef = useFocusOnMount<HTMLButtonElement>()

  async function confirm() {
    if (busy) {
      return
    }
    setBusy(true)
    setError(null)
    try {
      // Resolves only if the elevated instance started; this window is then
      // closed by the backend. A refused UAC prompt rejects instead.
      await restartAsAdmin()
    } catch (cause) {
      const message = errorMessage(cause)
      log.error('restart as administrator failed', { error: message })
      setError(message)
      setBusy(false)
    }
  }

  return (
    <ConfirmDialogShell
      label="Administrator permissions needed"
      title="Administrator permissions needed"
      confirmLabel="Restart as administrator"
      confirmRef={confirmRef}
      confirmDisabled={busy}
      error={error}
      onConfirm={() => void confirm()}
      onClose={close}
    >
      <DialogText>
        Windows refused this operation. Restart the application as an administrator, then try again.
      </DialogText>
      <DialogList items={[dialog.message]} />
    </ConfirmDialogShell>
  )
}

function CalculateAllSizesDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'calculateAllSizes' }>
}) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()

  function confirm() {
    const calculate = () => usePanesStore.getState().calculateAllFolderSizes(dialog.paneId)
    void run(calculate, { message: 'calculate all folder sizes failed' })
  }

  return (
    <ConfirmDialogShell
      label="Confirm calculate all folder sizes"
      title="Calculate all folder sizes?"
      confirmLabel="Calculate"
      confirmRef={confirmRef}
      confirmDisabled={busy}
      confirmOnEnter
      error={error}
      onConfirm={confirm}
      onClose={close}
    >
      <DialogText>
        This scans every folder in this pane to compute its size. It may take a while for folders
        with many files.
      </DialogText>
    </ConfirmDialogShell>
  )
}
