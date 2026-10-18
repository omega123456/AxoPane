import { useEffect, useRef, useState } from 'react'
import { Button, CheckboxFlag, TextField } from '@/components/controls'
import { DialogList, DialogShell, DialogText } from '@/components/dialogs/DialogShell'
import type { ConflictInfo } from '@/lib/types/ipc'

type ConflictDialogProps = {
  conflict: ConflictInfo
  onResolve: (
    resolution: 'replace' | 'skip' | 'rename',
    applyToAll: boolean,
    renameTo: string | null,
  ) => void
}

export function ConflictDialog({ conflict, onResolve }: ConflictDialogProps) {
  const [applyToAll, setApplyToAll] = useState(false)
  const [renaming, setRenaming] = useState(false)
  const [renameValue, setRenameValue] = useState(conflict.name)
  const skipRef = useRef<HTMLButtonElement>(null)
  const renameInputRef = useRef<HTMLInputElement>(null)

  // The Skip button is the default action and receives initial focus (Enter=Skip).
  useEffect(() => {
    skipRef.current?.focus()
  }, [])

  useEffect(() => {
    if (renaming) {
      renameInputRef.current?.focus()
      renameInputRef.current?.select()
    }
  }, [renaming])

  function resolve(resolution: 'replace' | 'skip' | 'rename') {
    if (resolution === 'rename') {
      const trimmed = renameValue.trim()
      onResolve('rename', applyToAll, trimmed.length > 0 ? trimmed : null)
      return
    }
    onResolve(resolution, applyToAll, null)
  }

  function onKeyDown(event: React.KeyboardEvent) {
    if (renaming) {
      if (event.key === 'Enter') {
        event.preventDefault()
        resolve('rename')
      } else if (event.key === 'Escape') {
        event.preventDefault()
        setRenaming(false)
      }
      return
    }

    switch (event.key) {
      case 'Enter':
        event.preventDefault()
        resolve('skip')
        break
      case 'Escape':
        event.preventDefault()
        resolve('skip')
        break
      case 'r':
      case 'R':
        event.preventDefault()
        resolve('replace')
        break
      case 'n':
      case 'N':
        event.preventDefault()
        setRenaming(true)
        break
      case 'a':
      case 'A':
        event.preventDefault()
        setApplyToAll((value) => !value)
        break
      default:
        break
    }
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
        <>
          <Button
            variant="primary"
            title="Replace (R)"
            aria-keyshortcuts="R"
            onClick={() => resolve('replace')}
          >
            Replace
          </Button>
          <Button
            title={renaming ? 'Confirm rename (Enter)' : 'Rename (N)'}
            aria-keyshortcuts={renaming ? 'Enter' : 'N'}
            onClick={() => (renaming ? resolve('rename') : setRenaming(true))}
          >
            {renaming ? 'Confirm rename' : 'Rename'}
          </Button>
          <Button
            ref={skipRef}
            title="Skip (Enter)"
            aria-keyshortcuts="Enter"
            onClick={() => resolve('skip')}
          >
            Skip
          </Button>
        </>
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
