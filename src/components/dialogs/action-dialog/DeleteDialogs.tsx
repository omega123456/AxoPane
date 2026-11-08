import { DialogList, DialogText } from '@/components/dialogs/DialogShell'
import { isMacPlatform } from '@/lib/keymap'
import { deleteFromTrash, emptyTrash } from '@/lib/ipc/commands'
import { startOp } from '@/lib/queue-commands'
import type { ActionDialog as ActionDialogState, DeleteTarget } from '@/stores/action-dialog-store'
import { usePanesStore } from '@/stores/panes-store'
import { ConfirmDialogShell } from './ConfirmDialogShell'
import { itemCountLabel, targetSummary } from './action-dialog-paths'
import { useFocusOnMount } from '@/lib/use-focus-on-mount'
import { useActionDialogSubmit } from './use-action-dialog-submit'

const DELETE_HAS_NO_DESTINATION_DIR = ''
const UNKNOWN_SIZE_BYTES = 0

function enqueuePermanentDelete(targets: readonly DeleteTarget[]) {
  return startOp({
    kind: 'delete',
    destinationDir: DELETE_HAS_NO_DESTINATION_DIR,
    items: targets.map((target) => ({
      sourcePath: target.path,
      name: target.name,
      sizeBytes: target.sizeBytes ?? UNKNOWN_SIZE_BYTES,
    })),
  })
}

export function DeleteDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'delete' }>
}) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()
  const count = dialog.targets.length

  function confirm() {
    void run(() => enqueuePermanentDelete(dialog.targets), { message: 'delete failed' })
  }

  return (
    <ConfirmDialogShell
      label="Confirm delete"
      title={`Delete ${itemCountLabel(count)} permanently?`}
      confirmLabel="Delete"
      confirmRef={confirmRef}
      confirmDisabled={busy}
      error={error}
      onConfirm={confirm}
      onClose={close}
    >
      <DialogText>
        {count === 1 ? 'This item skips' : 'These items skip'} the{' '}
        {isMacPlatform() ? 'Trash' : 'Recycle Bin'}. You can't undo this.
      </DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
    </ConfirmDialogShell>
  )
}

export function EmptyTrashDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'emptyTrash' }>
}) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()

  async function emptyAndReload() {
    await emptyTrash()
    await usePanesStore.getState().reloadPane(dialog.paneId)
  }

  return (
    <ConfirmDialogShell
      label="Confirm empty trash"
      title={`Empty trash (${itemCountLabel(dialog.count)})?`}
      confirmLabel="Empty trash"
      confirmRef={confirmRef}
      confirmDisabled={busy}
      error={error}
      onConfirm={() => void run(emptyAndReload, { message: 'empty trash failed' })}
      onClose={close}
    >
      <DialogText>
        This permanently deletes everything currently in the trash. You can't undo this.
      </DialogText>
    </ConfirmDialogShell>
  )
}

export function DeleteFromTrashDialog({
  dialog,
}: {
  dialog: Extract<ActionDialogState, { kind: 'deleteFromTrash' }>
}) {
  const { close, busy, error, run } = useActionDialogSubmit()
  const confirmRef = useFocusOnMount<HTMLButtonElement>()
  const count = dialog.targets.length

  async function deleteAndReload() {
    await deleteFromTrash({ ids: dialog.targets.map((target) => target.id) })
    await usePanesStore.getState().reloadPane(dialog.paneId)
  }

  return (
    <ConfirmDialogShell
      label="Confirm delete from trash"
      title={`Delete ${itemCountLabel(count)} from trash?`}
      confirmLabel="Delete"
      confirmRef={confirmRef}
      confirmDisabled={busy}
      error={error}
      onConfirm={() => void run(deleteAndReload, { message: 'delete from trash failed' })}
      onClose={close}
    >
      <DialogText>
        This permanently deletes the selected {count === 1 ? 'item' : 'items'} from the trash. You
        can't undo this.
      </DialogText>
      <DialogList items={targetSummary(dialog.targets)} />
    </ConfirmDialogShell>
  )
}
