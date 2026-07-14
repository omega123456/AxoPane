import { PieChartIcon, RefreshIcon, SearchIcon } from '@/components/icons'
import type { PaneState } from '@/types/pane'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { autoFolderSizeDisabledForPane, usePanesStore } from '@/stores/panes-store'
import { PaneViewMenu } from './PaneViewMenu'

type PaneToolbarProps = {
  pane: PaneState
  isActive: boolean
}

export function PaneToolbar({ pane, isActive }: PaneToolbarProps) {
  const setFilterDraft = usePanesStore((state) => state.setFilterDraft)
  const clearFilter = usePanesStore((state) => state.clearFilter)
  const refreshEverything = usePanesStore((state) => state.refreshEverything)
  const everythingAvailable = usePanesStore((state) => state.everythingStatus?.isAvailable ?? false)
  const autoFolderSize = useConfigStore((state) => state.autoFolderSize)
  const openActionDialog = useActionDialogStore((state) => state.open)
  const showCalculateAllSizes =
    !everythingAvailable || !autoFolderSize || autoFolderSizeDisabledForPane(pane.entries)

  return (
    <div
      role="toolbar"
      aria-label={`${pane.title} toolbar`}
      className="flex h-crumb min-w-0 items-center gap-2 border-b border-light-border bg-light-surface px-3 dark:border-dark-border dark:bg-dark-surface"
    >
      <button
        type="button"
        aria-label={`Refresh ${pane.title}`}
        title="Refresh"
        onClick={() => void refreshEverything(pane.id)}
        className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
      >
        <RefreshIcon className="h-3.5 w-3.5" />
      </button>
      {showCalculateAllSizes ? (
        <button
          type="button"
          aria-label={`Calculate all folder sizes in ${pane.title}`}
          title="Calculate all folder sizes"
          onClick={() => openActionDialog({ kind: 'calculateAllSizes', paneId: pane.id })}
          className="inline-flex h-7 w-7 shrink-0 cursor-pointer items-center justify-center rounded-tab text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          <PieChartIcon className="h-3.5 w-3.5" />
        </button>
      ) : null}
      <PaneViewMenu paneId={pane.id} />
      <span className="ml-auto shrink-0 font-mono text-uxs text-light-text-muted dark:text-dark-text-muted">
        {pane.entries.length} items
      </span>
      <label
        className={`flex h-8 min-w-0 w-search items-center gap-2 rounded-tab border px-2 ${
          isActive ? 'border-accent-blue-border' : 'border-light-border dark:border-dark-border'
        } bg-light-panel dark:bg-dark-panel`}
      >
        <SearchIcon className="h-3.5 w-3.5 shrink-0 text-light-text-muted dark:text-dark-text-muted" />
        <input
          aria-label={`${pane.title} filter`}
          value={pane.filterDraft}
          onChange={(event) => setFilterDraft(pane.id, event.target.value)}
          onKeyDown={(event) => {
            if (event.key === 'Escape') {
              event.preventDefault()
              clearFilter(pane.id)
              document.querySelector<HTMLElement>(`[data-pane-id="${pane.id}"]`)?.focus()
            }
          }}
          placeholder="Filter current folder"
          className="min-w-0 flex-1 select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-faint dark:text-dark-text dark:placeholder:text-dark-text-faint"
        />
      </label>
    </div>
  )
}
