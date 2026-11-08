import { useState, type Ref } from 'react'
import { Button, CheckboxFlag, TextField } from '@/components/controls'
import { DialogList, DialogShell, DialogText } from '@/components/dialogs/DialogShell'
import type { ConflictInfo } from '@/lib/types/ipc'
import { useFocusOnMount, useSelectWhen } from '@/lib/use-focus-on-mount'
import { caseInsensitiveLetterKeys, handleKeyByTable, type KeyHandlerTable } from '@/lib/key-table'

type ConflictResolution = 'replace' | 'skip' | 'rename'

type ConflictDialogProps = {
  conflict: ConflictInfo
  onResolve: (resolution: ConflictResolution, applyToAll: boolean, renameTo: string | null) => void
}

type ConflictFooterProps = {
  renaming: boolean
  skipRef: Ref<HTMLButtonElement>
  onResolve: (resolution: ConflictResolution) => void
  onStartRename: () => void
}

function ConflictFooter({ renaming, skipRef, onResolve, onStartRename }: ConflictFooterProps) {
  return (
    <>
      <Button
        variant="primary"
        title="Replace (R)"
        aria-keyshortcuts="R"
        onClick={() => onResolve('replace')}
      >
        Replace
      </Button>
      <Button
        title={renaming ? 'Confirm rename (Enter)' : 'Rename (N)'}
        aria-keyshortcuts={renaming ? 'Enter' : 'N'}
        onClick={() => (renaming ? onResolve('rename') : onStartRename())}
      >
        {renaming ? 'Confirm rename' : 'Rename'}
      </Button>
      <Button
        ref={skipRef}
        title="Skip (Enter)"
        aria-keyshortcuts="Enter"
        onClick={() => onResolve('skip')}
      >
        Skip
      </Button>
    </>
  )
}

type ConflictKeyActions = {
  resolve: (resolution: ConflictResolution) => void
  setRenaming: (renaming: boolean) => void
  toggleApplyToAll: () => void
}

function conflictKeyTable(renaming: boolean, actions: ConflictKeyActions): KeyHandlerTable {
  if (renaming) {
    return { Enter: () => actions.resolve('rename'), Escape: () => actions.setRenaming(false) }
  }
  return {
    Enter: () => actions.resolve('skip'),
    Escape: () => actions.resolve('skip'),
    ...caseInsensitiveLetterKeys({
      r: () => actions.resolve('replace'),
      n: () => actions.setRenaming(true),
      a: actions.toggleApplyToAll,
    }),
  }
}

export function ConflictDialog({ conflict, onResolve }: ConflictDialogProps) {
  const [applyToAll, setApplyToAll] = useState(false)
  const [renaming, setRenaming] = useState(false)
  const [renameValue, setRenameValue] = useState(conflict.name)
  const skipRef = useFocusOnMount<HTMLButtonElement>()
  const renameInputRef = useSelectWhen(renaming)

  function resolve(resolution: ConflictResolution) {
    if (resolution === 'rename') {
      const trimmed = renameValue.trim()
      onResolve('rename', applyToAll, trimmed.length > 0 ? trimmed : null)
      return
    }
    onResolve(resolution, applyToAll, null)
  }

  function onKeyDown(event: React.KeyboardEvent) {
    const toggleApplyToAll = () => setApplyToAll((value) => !value)
    handleKeyByTable(event, conflictKeyTable(renaming, { resolve, setRenaming, toggleApplyToAll }))
  }

  return (
    <DialogShell
      label="Resolve file conflict"
      title="File already exists"
      dismissLabel="Dismiss conflict"
      onDismiss={() => resolve('skip')}
      onKeyDown={onKeyDown}
      footerColumns={3}
      footer={
        <ConflictFooter
          renaming={renaming}
          skipRef={skipRef}
          onResolve={resolve}
          onStartRename={() => setRenaming(true)}
        />
      }
    >
      <DialogText>
        “{conflict.name}” already exists in the destination. Choose how to continue.
      </DialogText>
      <DialogList items={[conflict.destinationPath]} />
      {renaming ? (
        <TextField
          ref={renameInputRef}
          value={renameValue}
          onChange={(event) => setRenameValue(event.target.value)}
          aria-label="New name"
        />
      ) : null}
      <CheckboxFlag
        checked={applyToAll}
        onChange={setApplyToAll}
        title="Apply to all conflicts in this transfer"
        tooltip="Apply to all (A)"
        keyShortcut="A"
      />
    </DialogShell>
  )
}
