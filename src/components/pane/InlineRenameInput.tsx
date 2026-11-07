import type { RefObject } from 'react'
import { handleKeyByTable } from '@/lib/key-table'
import type { FileRowActions } from './FileRow'

type RenameActions = Pick<
  FileRowActions,
  'onRenameBlur' | 'onRenameChange' | 'onRenameSubmit' | 'onRenameCancel'
>

export function InlineRenameInput({
  inputRef,
  entryName,
  value,
  busy,
  actions,
  className,
}: {
  inputRef: RefObject<HTMLInputElement | null>
  entryName: string
  value: string
  busy: boolean
  actions: RenameActions
  className: string
}) {
  return (
    <input
      ref={inputRef}
      autoComplete="off"
      aria-label={`Rename ${entryName}`}
      value={value}
      disabled={busy}
      onMouseDown={(event) => event.stopPropagation()}
      onBlur={actions.onRenameBlur}
      onChange={(event) => actions.onRenameChange(event.target.value)}
      onKeyDown={(event) => {
        event.stopPropagation()
        handleKeyByTable(event, {
          Enter: () => actions.onRenameSubmit(),
          Escape: () => actions.onRenameCancel(),
        })
      }}
      className={`min-w-0 select-text rounded-control border border-light-accent dark:border-dark-accent bg-light-base px-2 py-1 font-mono text-caption text-light-text outline-none dark:bg-dark-base dark:text-dark-text ${className}`}
    />
  )
}
