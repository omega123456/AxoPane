import { ChevronDownIcon, Grid2X2Icon, ImagesIcon, ListIcon } from '@/components/icons'
import { MenuPopover, type MenuPopoverRadioItem } from '@/components/controls'
import { paneViewMetadata, paneViewModes } from '@/lib/pane-view'
import type { PaneId } from '@/stores/tabs-store'
import { useTabsStore } from '@/stores/tabs-store'
import { FOCUS_RING } from '@/lib/focus-ring'

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
    icon: (() => {
      const ModeIcon = modeIcons[mode]
      return <ModeIcon className="size-4" />
    })(),
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
          title="View"
          className={`inline-flex h-8 cursor-pointer items-center gap-1.5 whitespace-nowrap rounded-control px-2 text-row text-light-text hover:bg-light-hover active:bg-light-press dark:text-dark-text dark:hover:bg-dark-hover dark:active:bg-dark-press ${FOCUS_RING}`}
        >
          <Icon className="size-4" />
          <span>{paneViewMetadata[viewMode].label}</span>
          <ChevronDownIcon className="size-3 text-light-text-secondary dark:text-dark-text-secondary" />
        </button>
      )}
    />
  )
}
