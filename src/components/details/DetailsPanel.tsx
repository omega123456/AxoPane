import type { ReactNode } from 'react'
import { CopyIcon, FolderOpenIcon, InfoIcon } from '@/components/icons'
import { executeCommand } from '@/lib/commands'
import { logFrontend } from '@/lib/app-log-commands'
import { usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'

type DetailsPanelProps = {
  paneId: PaneId
}

export function DetailsPanel({ paneId }: DetailsPanelProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const entry = pane.entries.find((item) => item.id === pane.focusedEntryId) ?? pane.entries[0]

  if (!entry) {
    return (
      <aside className="w-details shrink-0 border-l border-light-border bg-light-panel p-4 dark:border-dark-border dark:bg-dark-panel">
        <p className="text-row text-light-text-muted dark:text-dark-text-muted">No selection</p>
      </aside>
    )
  }

  return (
    <aside className="flex w-details shrink-0 flex-col border-l border-light-border bg-light-panel dark:border-dark-border dark:bg-dark-panel">
      <div className="border-b border-light-border px-4 py-3 dark:border-dark-border">
        <p className="font-mono text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
          Details
        </p>
      </div>
      <div className="min-h-0 flex-1 overflow-auto p-4">
        <div className="flex h-36 items-center justify-center rounded-window border border-dashed border-light-border-strong bg-light-surface dark:border-dark-border-strong dark:bg-dark-surface">
          <span className="rounded-tab bg-light-header px-3 py-1 font-mono text-uxs text-light-text-muted dark:bg-dark-header dark:text-dark-text-muted">
            Preview placeholder
          </span>
        </div>
        <h2 className="mt-4 break-all text-sm font-semibold text-light-text dark:text-dark-text">
          {entry.name}
        </h2>
        <dl className="mt-4 space-y-2 text-row">
          <div className="flex justify-between gap-3 border-b border-light-border py-2 dark:border-dark-border">
            <dt className="text-light-text-muted dark:text-dark-text-muted">Type</dt>
            <dd className="text-right text-light-text-soft dark:text-dark-text-soft">
              {entry.typeLabel}
            </dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-border py-2 dark:border-dark-border">
            <dt className="text-light-text-muted dark:text-dark-text-muted">Path</dt>
            <dd className="text-right text-light-text-soft dark:text-dark-text-soft">
              {entry.path}
            </dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-border py-2 dark:border-dark-border">
            <dt className="text-light-text-muted dark:text-dark-text-muted">Modified</dt>
            <dd className="text-right text-light-text-soft dark:text-dark-text-soft">
              {entry.modifiedAt ?? '—'}
            </dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-border py-2 dark:border-dark-border">
            <dt className="text-light-text-muted dark:text-dark-text-muted">Created</dt>
            <dd className="text-right text-light-text-soft dark:text-dark-text-soft">
              {entry.createdAt ?? '—'}
            </dd>
          </div>
        </dl>
      </div>
      <div className="grid grid-cols-3 gap-2 border-t border-light-border p-3 dark:border-dark-border">
        <ActionButton label="Open" onClick={() => executeCommand('open', paneId, entry.id)}>
          <FolderOpenIcon className="h-4 w-4" />
        </ActionButton>
        <ActionButton
          label="Copy path"
          onClick={async () => {
            await navigator.clipboard?.writeText(entry.path)
            logFrontend('Copied path', { path: entry.path })
          }}
        >
          <CopyIcon className="h-4 w-4" />
        </ActionButton>
        <ActionButton
          label="Properties"
          onClick={() => {
            logFrontend('Properties requested', { path: entry.path, attributes: entry.attributes })
          }}
        >
          <InfoIcon className="h-4 w-4" />
        </ActionButton>
      </div>
    </aside>
  )
}

type ActionButtonProps = {
  children: ReactNode
  label: string
  onClick: () => void
}

function ActionButton({ children, label, onClick }: ActionButtonProps) {
  return (
    <button
      type="button"
      onClick={onClick}
      className="inline-flex items-center justify-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 py-2 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text-soft"
    >
      {children}
      <span>{label}</span>
    </button>
  )
}
