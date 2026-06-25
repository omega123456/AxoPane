import { useMemo } from 'react'
import { columnDefinitions } from '@/lib/columns'
import type { PaneState } from '@/types/pane'
import { ChevronDownIcon } from '@/components/icons'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'

type HeaderRowProps = {
  pane: PaneState
}

export function HeaderRow({ pane }: HeaderRowProps) {
  const setSort = usePanesStore((state) => state.setSort)
  const configuredColumns = useLayoutStore((state) => state.columns)
  const columns = useMemo(
    () => configuredColumns.filter((column) => column.visible),
    [configuredColumns],
  )

  return (
    <div className="flex h-headrow items-center gap-3 border-b border-light-border bg-light-header px-3 text-uxs uppercase tracking-wide text-light-text-muted dark:border-dark-border dark:bg-dark-header dark:text-dark-text-muted">
      {columns.map((column) => {
        const active = pane.sortKey === column.key
        const definition = columnDefinitions[column.key]
        return (
          <button
            key={column.key}
            type="button"
            onClick={() => void setSort(pane.id, column.key)}
            className={`${definition.className} inline-flex items-center gap-1 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border`}
          >
            <span>{definition.label}</span>
            {active ? (
              <ChevronDownIcon
                className={`h-3.5 w-3.5 ${pane.sortDirection === 'asc' ? 'rotate-180' : ''} text-accent-blue-light dark:text-accent-blue`}
              />
            ) : null}
          </button>
        )
      })}
    </div>
  )
}
