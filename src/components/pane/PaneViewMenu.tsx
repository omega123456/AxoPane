import { ChevronDownIcon, Grid2X2Icon, ImagesIcon, ListIcon } from '@/components/icons'
import { MenuPopover, type MenuPopoverRadioItem } from '@/components/controls'
import { paneViewMetadata, paneViewModes } from '@/lib/pane-view'
import type { PaneId } from '@/stores/tabs-store'
import { useTabsStore } from '@/stores/tabs-store'

const modeIcons = { details: ListIcon, icons: Grid2X2Icon, thumbnails: ImagesIcon } as const

export function PaneViewMenu({ paneId }: { paneId: PaneId }) {
  const viewMode = useTabsStore((state) => {
    const pane = state.panes[paneId]
    return pane.tabs[pane.activeTabIndex]?.viewMode ?? 'details'
  })
  const patchActiveTab = useTabsStore((state) => state.patchActiveTab)
  const Icon = modeIcons[viewMode]
  const items: MenuPopoverRadioItem[] = paneViewModes.map((mode) => ({
    id: mode,
    label: paneViewMetadata[mode].label,
    icon: (() => { const ModeIcon = modeIcons[mode]; return <ModeIcon className="size-4" /> })(),
    checked: mode === viewMode,
    onSelect: () => patchActiveTab(paneId, { viewMode: mode }),
  }))

  return (
    <MenuPopover
      ariaLabel="View options"
      radio
      items={items}
      trigger={({ ref, expanded, controls, toggle, onTriggerKeyDown }) => (
        <button
          ref={ref}
          type="button"
          aria-label={`View: ${paneViewMetadata[viewMode].label}`}
          aria-haspopup="menu"
          aria-expanded={expanded}
          aria-controls={controls}
          onClick={toggle}
          onKeyDown={onTriggerKeyDown}
          className="inline-flex h-7 items-center gap-1 rounded-tab px-2 text-row text-light-text-soft hover:bg-light-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft dark:hover:bg-dark-hover"
        >
          <Icon className="size-3.5" />
          <span>View</span>
          <ChevronDownIcon className="size-3" />
        </button>
      )}
    />
  )
}
