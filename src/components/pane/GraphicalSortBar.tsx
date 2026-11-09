import type { ComponentProps } from 'react'
import { ChevronDownIcon } from '@/components/icons'
import { MenuPopover, type MenuPopoverRadioItem } from '@/components/controls'
import { columnDefinitions } from '@/lib/columns'
import type { SortKey } from '@/lib/types/ipc'
import type { PaneState } from '@/types/pane'
import { usePanesStore } from '@/stores/panes-store'
import { FOCUS_RING } from '@/lib/focus-ring'
import { TEXT_SECONDARY, HOVER_ON_POINTER } from '@/lib/class-names'

const sortKeys: SortKey[] = ['name', 'size', 'items', 'type', 'modified', 'created']

const directionLabels = { asc: 'Ascending', desc: 'Descending' } as const

type SortTriggerProps = Parameters<ComponentProps<typeof MenuPopover>['trigger']>[0] & {
  ariaLabel: string
  label: string
}

function SortTrigger({
  ref,
  expanded,
  controls,
  toggle,
  onTriggerKeyDown,
  ariaLabel,
  label,
}: SortTriggerProps) {
  return (
    <button
      ref={ref}
      type="button"
      aria-label={ariaLabel}
      aria-haspopup="menu"
      aria-expanded={expanded}
      aria-controls={controls}
      onClick={toggle}
      onKeyDown={onTriggerKeyDown}
      className={`inline-flex items-center gap-1 rounded-control px-2 py-1 ${HOVER_ON_POINTER} ${FOCUS_RING}`}
    >
      {label}
      <ChevronDownIcon className="size-3" />
    </button>
  )
}

function directionItem(
  pane: PaneState,
  direction: PaneState['sortDirection'],
  setSort: (paneId: PaneState['id'], key: SortKey) => unknown,
): MenuPopoverRadioItem {
  return {
    id: direction,
    label: directionLabels[direction],
    checked: pane.sortDirection === direction,
    onSelect: () => {
      if (pane.sortDirection !== direction) void setSort(pane.id, pane.sortKey)
    },
  }
}

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
  const directions = [directionItem(pane, 'asc', setSort), directionItem(pane, 'desc', setSort)]
  const fieldLabel = columnDefinitions[pane.sortKey].label
  const directionLabel = directionLabels[pane.sortDirection]
  return (
    <div
      className={`flex h-headrow items-center gap-2 border-b border-light-divider px-2.75 text-caption ${TEXT_SECONDARY} dark:border-dark-divider`}
    >
      <span className="shrink-0 whitespace-nowrap">Sort by</span>
      <MenuPopover
        ariaLabel="Sort field"
        radio
        items={fields}
        trigger={(triggerProps) => (
          <SortTrigger
            {...triggerProps}
            ariaLabel={`Sort field: ${fieldLabel}`}
            label={fieldLabel}
          />
        )}
      />
      <MenuPopover
        ariaLabel="Sort direction"
        radio
        items={directions}
        trigger={(triggerProps) => (
          <SortTrigger
            {...triggerProps}
            ariaLabel={`Sort direction: ${directionLabel}`}
            label={directionLabel}
          />
        )}
      />
    </div>
  )
}
