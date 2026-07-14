import { useEffect, useRef, useState } from 'react'
import { AlertTriangleIcon } from '@/components/icons'
import type { ConflictInfo } from '@/lib/types/ipc'

type ConflictDialogProps = {
  conflict: ConflictInfo
  onResolve: (
    resolution: 'replace' | 'skip' | 'rename',
    applyToAll: boolean,
    renameTo: string | null,
  ) => void
}

function isWindows() {
  if (typeof navigator === 'undefined') {
    return false
  }
  return navigator.userAgent.includes('Windows')
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
    <div
      role="dialog"
      aria-modal="true"
      aria-label="Resolve file conflict"
      onKeyDown={onKeyDown}
      className="fixed inset-0 z-40 flex items-center justify-center"
    >
      <button
        type="button"
        aria-label="Dismiss conflict"
        tabIndex={-1}
        onClick={() => resolve('skip')}
        className="absolute inset-0 cursor-default bg-dark-backdrop/40"
      />
      <div className="relative w-conflict overflow-hidden rounded-window border border-light-border-strong bg-light-surface shadow-window dark:border-dark-border-strong dark:bg-dark-surface">
        <div className="flex items-start gap-3 border-b border-light-border p-4 dark:border-dark-border">
          <AlertTriangleIcon className="mt-0.5 h-5 w-5 shrink-0 text-accent-amber" />
          <div className="min-w-0">
            <div className="text-sm font-semibold text-light-text dark:text-dark-text">
              File already exists
            </div>
            <div className="mt-1 break-all font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
              {conflict.destinationPath}
            </div>
          </div>
        </div>

        <div className="p-4">
          <p className="text-row text-light-text-soft dark:text-dark-text-soft">
            “{conflict.name}” already exists in the destination. Choose how to continue.
          </p>

          {renaming ? (
            <input
              ref={renameInputRef}
              value={renameValue}
              onChange={(event) => setRenameValue(event.target.value)}
              aria-label="New name"
              className="mt-3 h-9 w-full select-text rounded-tab border border-accent-blue-border bg-light-window px-3 font-mono text-xs text-light-text outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:bg-dark-window dark:text-dark-text"
            />
          ) : null}

          <label className="mt-3 flex items-center gap-2 text-uxs text-light-text-muted dark:text-dark-text-muted">
            <input
              type="checkbox"
              checked={applyToAll}
              onChange={(event) => setApplyToAll(event.target.checked)}
              className="h-3.5 w-3.5 rounded focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            />
            Apply to all conflicts in this transfer
            <span className="font-mono text-light-text-faint dark:text-dark-text-faint">(A)</span>
          </label>
        </div>

        <div
          className={`flex gap-2 border-t border-light-border p-4 dark:border-dark-border ${
            isWindows() ? '' : 'flex-row-reverse'
          }`}
        >
          <button
            type="button"
            onClick={() => resolve('replace')}
            className="rounded-md bg-accent-blue-soft px-4 py-2 text-xs font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
          >
            Replace <span className="font-mono opacity-70">(R)</span>
          </button>
          <button
            type="button"
            onClick={() => (renaming ? resolve('rename') : setRenaming(true))}
            className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            {renaming ? 'Confirm rename' : 'Rename'}{' '}
            <span className="font-mono opacity-70">(N)</span>
          </button>
          <button
            ref={skipRef}
            type="button"
            onClick={() => resolve('skip')}
            className="rounded-md border border-light-border px-4 py-2 text-xs text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border hover:bg-light-hover dark:border-dark-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
          >
            Skip <span className="font-mono opacity-70">(Enter)</span>
          </button>
        </div>
      </div>
    </div>
  )
}
