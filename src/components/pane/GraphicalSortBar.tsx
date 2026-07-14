import { ChevronDownIcon } from '@/components/icons'
import { MenuPopover, type MenuPopoverRadioItem } from '@/components/controls'
import { columnDefinitions } from '@/lib/columns'
import type { SortKey } from '@/lib/types/ipc'
import type { PaneState } from '@/types/pane'
import { usePanesStore } from '@/stores/panes-store'

const sortKeys: SortKey[] = ['name', 'size', 'items', 'type', 'modified', 'created']

export function GraphicalSortBar({ pane }: { pane: PaneState }) {
  const setSort = usePanesStore((state) => state.setSort)
  const fields: MenuPopoverRadioItem[] = sortKeys.map((key) => ({
    id: key,
    label: columnDefinitions[key].label,
    checked: key === pane.sortKey,
    onSelect: () => {
      if (key !== pane.sortKey) void setSort(pane.id, key)
    },
  }))
  const directions: MenuPopoverRadioItem[] = [
    {
      id: 'asc',
      label: 'Ascending',
      checked: pane.sortDirection === 'asc',
      onSelect: () => {
        if (pane.sortDirection !== 'asc') void setSort(pane.id, pane.sortKey)
      },
    },
    {
      id: 'desc',
      label: 'Descending',
      checked: pane.sortDirection === 'desc',
      onSelect: () => {
        if (pane.sortDirection !== 'desc') void setSort(pane.id, pane.sortKey)
      },
    },
  ]
  return (
    <div className="flex h-headrow items-center gap-2 border-b border-light-border bg-light-header px-3 text-row text-light-text-soft dark:border-dark-border dark:bg-dark-header dark:text-dark-text-soft">
      <span className="text-light-text-muted dark:text-dark-text-muted">Sort by</span>
      <MenuPopover
        ariaLabel="Sort field"
        radio
        items={fields}
        trigger={({ ref, expanded, controls, toggle, onTriggerKeyDown }) => (
          <button
            ref={ref}
            type="button"
            aria-label={`Sort field: ${columnDefinitions[pane.sortKey].label}`}
            aria-haspopup="menu"
            aria-expanded={expanded}
            aria-controls={controls}
            onClick={toggle}
            onKeyDown={onTriggerKeyDown}
            className="inline-flex items-center gap-1 rounded-tab px-2 py-1 hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:hover:bg-dark-hover"
          >
            {columnDefinitions[pane.sortKey].label}
            <ChevronDownIcon className="size-3" />
          </button>
        )}
      />
      <MenuPopover
        ariaLabel="Sort direction"
        radio
        items={directions}
        trigger={({ ref, expanded, controls, toggle, onTriggerKeyDown }) => (
          <button
            ref={ref}
            type="button"
            aria-label={`Sort direction: ${pane.sortDirection === 'asc' ? 'Ascending' : 'Descending'}`}
            aria-haspopup="menu"
            aria-expanded={expanded}
            aria-controls={controls}
            onClick={toggle}
            onKeyDown={onTriggerKeyDown}
            className="inline-flex items-center gap-1 rounded-tab px-2 py-1 hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:hover:bg-dark-hover"
          >
            {pane.sortDirection === 'asc' ? 'Ascending' : 'Descending'}
            <ChevronDownIcon className="size-3" />
          </button>
        )}
      />
    </div>
  )
}
