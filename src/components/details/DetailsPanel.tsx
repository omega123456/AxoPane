import type { ReactNode } from 'react'
import { CopyIcon, FolderOpenIcon, InfoIcon } from '@/components/icons'
import { executeCommand } from '@/lib/commands'
import { copyPathsToClipboard } from '@/lib/path-clipboard'
import { showPropertiesDialog, toPropertiesDialogItem } from '@/lib/properties-commands'
import { indexEntriesById, usePanesStore } from '@/stores/panes-store'
import type { PaneId } from '@/types/pane'
import { FOCUS_RING } from '@/lib/focus-ring'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'

type DetailsPanelProps = {
  paneId: PaneId
}

export function DetailsPanel({ paneId }: DetailsPanelProps) {
  const pane = usePanesStore((state) => state.panes[paneId])
  const entry =
    (pane.focusedEntryId ? indexEntriesById(pane.entries).get(pane.focusedEntryId) : undefined) ??
    pane.entries[0]

  if (!entry) {
    return (
      <aside className="w-details shrink-0 border-l border-light-divider bg-light-base p-4 dark:border-dark-divider dark:bg-dark-base">
        <p className={`text-row ${TEXT_SECONDARY}`}>No selection</p>
      </aside>
    )
  }

  return (
    <aside className="flex w-details shrink-0 flex-col border-l border-light-divider bg-light-base dark:border-dark-divider dark:bg-dark-base">
      <div className="border-b border-light-divider px-4 py-3 dark:border-dark-divider">
        <p className={`font-mono text-caption ${TEXT_SECONDARY}`}>Details</p>
      </div>
      <div className={`min-h-0 flex-1 overflow-auto p-4 ${SCROLLBAR_THIN}`}>
        <div className="flex h-36 items-center justify-center rounded-overlay border border-dashed border-light-control-stroke bg-light-layer dark:border-dark-control-stroke dark:bg-dark-layer">
          <span
            className={`rounded-control bg-light-layer px-3 py-1 font-mono text-caption ${TEXT_SECONDARY} dark:bg-dark-layer`}
          >
            Preview placeholder
          </span>
        </div>
        <h2 className="mt-4 break-all text-body font-semibold text-light-text dark:text-dark-text">
          {entry.name}
        </h2>
        <dl className="mt-4 space-y-2 text-row">
          <div className="flex justify-between gap-3 border-b border-light-divider py-2 dark:border-dark-divider">
            <dt className={TEXT_SECONDARY}>Type</dt>
            <dd className={`text-right ${TEXT_SECONDARY}`}>{entry.typeLabel}</dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-divider py-2 dark:border-dark-divider">
            <dt className={TEXT_SECONDARY}>Path</dt>
            <dd className={`text-right ${TEXT_SECONDARY}`}>{entry.path}</dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-divider py-2 dark:border-dark-divider">
            <dt className={TEXT_SECONDARY}>Modified</dt>
            <dd className={`text-right ${TEXT_SECONDARY}`}>{entry.modifiedAt ?? '—'}</dd>
          </div>
          <div className="flex justify-between gap-3 border-b border-light-divider py-2 dark:border-dark-divider">
            <dt className={TEXT_SECONDARY}>Created</dt>
            <dd className={`text-right ${TEXT_SECONDARY}`}>{entry.createdAt ?? '—'}</dd>
          </div>
        </dl>
      </div>
      <div className="grid grid-cols-3 gap-2 border-t border-light-divider p-3 dark:border-dark-divider">
        <ActionButton label="Open" onClick={() => executeCommand('open', paneId, entry.id)}>
          <FolderOpenIcon className="h-4 w-4" />
        </ActionButton>
        <ActionButton
          label="Copy path"
          onClick={async () => {
            await copyPathsToClipboard([entry.path])
          }}
        >
          <CopyIcon className="h-4 w-4" />
        </ActionButton>
        <ActionButton
          label="Properties"
          onClick={() => void showPropertiesDialog([toPropertiesDialogItem(entry)])}
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
      className={`inline-flex items-center justify-center gap-2 rounded-control border border-light-divider bg-light-layer px-3 py-2 text-row ${TEXT_SECONDARY} dark:border-dark-divider dark:bg-dark-layer ${FOCUS_RING}`}
    >
      {children}
      <span>{label}</span>
    </button>
  )
}
