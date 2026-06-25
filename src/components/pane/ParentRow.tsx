import { ArrowUpIcon, FolderIcon } from '@/components/icons'

type ParentRowProps = {
  isActivePane: boolean
  isFocused: boolean
  onActivate: () => void
  onFocus: () => void
}

/**
 * Synthetic ".." row rendered at the top of a pane's listing whenever the
 * current folder has a parent. Activating it (Enter / double-click / click)
 * navigates to the parent directory. It is intentionally never selectable for
 * copy/move or folder-size operations — it carries no entry id.
 */
export function ParentRow({ isActivePane, isFocused, onActivate, onFocus }: ParentRowProps) {
  return (
    <button
      type="button"
      role="row"
      data-parent-row="true"
      aria-label="Go to parent folder"
      onClick={onFocus}
      onDoubleClick={onActivate}
      onKeyDown={(event) => {
        if (event.key === 'Enter') {
          event.preventDefault()
          onActivate()
        }
      }}
      className={`group flex h-row w-full items-center gap-3 border-b border-light-border px-3 text-row text-left focus-visible:outline-none dark:border-dark-border bg-light-surface dark:bg-dark-surface ${
        isFocused && isActivePane ? 'ring-2 ring-inset ring-accent-blue-border' : ''
      } hover:bg-light-hover dark:hover:bg-dark-hover`}
    >
      <span className="min-w-0 flex flex-1 items-center gap-2">
        <span className="relative flex h-4 w-4 shrink-0 items-center justify-center">
          <FolderIcon className="h-4 w-4 text-light-text-muted dark:text-dark-text-muted" />
          <ArrowUpIcon className="absolute h-2.5 w-2.5 text-accent-blue-light dark:text-accent-blue" />
        </span>
        <span className="truncate text-light-text-soft dark:text-dark-text-soft">..</span>
      </span>
      <span className="w-sizecol shrink-0 text-right font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        —
      </span>
      <span className="w-itemcol shrink-0 text-right font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        —
      </span>
      <span className="w-typecol shrink-0 truncate text-usm text-light-text-muted dark:text-dark-text-muted">
        Parent folder
      </span>
      <span className="w-modcol shrink-0 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        —
      </span>
    </button>
  )
}
