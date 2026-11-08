import { compactCount, folderSizeStatusLabel } from '@/lib/folder-size-status'
import {
  CheckCircleIcon,
  Columns2Icon,
  ContrastIcon,
  EyeOffIcon,
  HistoryIcon,
  LayersIcon,
  LayoutGridIcon,
  LoaderCircleIcon,
  PauseIcon,
  PieChartIcon,
  ZoomInIcon,
} from '@/components/icons'
import {
  SectionLabel,
  SegmentedControl,
  SelectField,
  SettingRow,
  ToggleSwitch,
} from '@/components/controls'
import type { FolderSizeIndexStatus } from '@/lib/types/ipc'
import { zoomLevels } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './section-props'

const STATUS_ICON_CLASS = 'h-3.5 w-3.5 shrink-0'

function FolderSizeStatusIcon({ state }: { state: FolderSizeIndexStatus['state'] }) {
  if (state === 'upToDate') {
    return <CheckCircleIcon className={STATUS_ICON_CLASS} />
  }
  if (state === 'paused') {
    return <PauseIcon className={STATUS_ICON_CLASS} />
  }
  return <LoaderCircleIcon className={`${STATUS_ICON_CLASS} animate-spin`} />
}

/** The macOS index's state, shown under the auto folder size description. */
function FolderSizeStatusLine({ status }: { status: FolderSizeIndexStatus }) {
  const label = folderSizeStatusLabel(status)
  const count =
    status.state === 'indexing' && status.itemsScanned !== undefined
      ? ` ${compactCount.format(status.itemsScanned)} items`
      : ''

  // The status region announces state transitions only, not count ticks.
  return (
    <span className="mt-1 flex items-center gap-1">
      <FolderSizeStatusIcon state={status.state} />
      <span aria-hidden="true">
        {label}
        {count}
      </span>
      <span role="status" className="sr-only">
        {label}
      </span>
    </span>
  )
}

export function LayoutSection(props: SectionProps) {
  return (
    <>
      <SectionLabel>View</SectionLabel>
      <ViewSettings {...props} />
      <SectionLabel>Layout</SectionLabel>
      <PaneLayoutSettings {...props} />
      <SessionSettings {...props} />
      <AutoFolderSizeSetting {...props} />
    </>
  )
}

function ViewSettings({ draft, updateDraft, updateLayout }: SectionProps) {
  return (
    <>
      <SettingRow
        icon={<EyeOffIcon className={iconClass} />}
        title="Show hidden files"
        description="Include hidden and system items in folder listings"
        control={
          <ToggleSwitch
            label="Show hidden files"
            checked={draft.showHiddenFiles}
            onChange={(value) => updateDraft((current) => ({ ...current, showHiddenFiles: value }))}
          />
        }
      />
      <SettingRow
        icon={<ContrastIcon className={iconClass} />}
        title="Theme"
        description="Match your desktop or pick one"
        control={
          <SegmentedControl
            ariaLabel="Theme"
            value={draft.theme}
            onChange={(value) =>
              updateDraft((current) => ({
                ...current,
                theme: value,
              }))
            }
            options={[
              { value: 'system', label: 'System' },
              { value: 'light', label: 'Light' },
              { value: 'dark', label: 'Dark' },
            ]}
          />
        }
      />
      <SettingRow
        icon={<ZoomInIcon className={iconClass} />}
        title="Zoom"
        description="Scale the whole interface"
        control={
          <SelectField
            ariaLabel="Zoom"
            value={draft.layout.zoom}
            onChange={(value) => updateLayout('zoom', value)}
            options={zoomLevels.map((level) => ({
              value: level,
              label: `${level}%`,
            }))}
          />
        }
      />
    </>
  )
}

function PaneLayoutSettings({ draft, updateLayout }: SectionProps) {
  return (
    <>
      <SettingRow
        icon={<Columns2Icon className={iconClass} />}
        title="Default pane mode"
        description="How panes open in a new window"
        control={
          <SegmentedControl
            ariaLabel="Default pane mode"
            value={draft.layout.defaultPaneMode}
            onChange={(value) => updateLayout('defaultPaneMode', value)}
            options={[
              { value: 'dual', label: 'Dual' },
              { value: 'single', label: 'Single' },
            ]}
          />
        }
      />
      <SettingRow
        icon={<LayoutGridIcon className={iconClass} />}
        title="Default view for new tabs"
        description="Used for new tabs and older sessions without a view"
        control={
          <SegmentedControl
            ariaLabel="Default view for new tabs"
            value={draft.layout.defaultViewMode}
            onChange={(value) => updateLayout('defaultViewMode', value)}
            options={[
              { value: 'details', label: 'Details' },
              { value: 'icons', label: 'Icons' },
              { value: 'thumbnails', label: 'Large thumbnails' },
            ]}
          />
        }
      />
    </>
  )
}

function SessionSettings({ draft, updateDraft, updateLayout }: SectionProps) {
  return (
    <>
      <SettingRow
        icon={<LayersIcon className={iconClass} />}
        title="Auto-expand active queue toasts"
        description="Open the full job queue automatically when new active work starts"
        control={
          <ToggleSwitch
            label="Auto-expand active queue toasts"
            checked={draft.autoExpandActiveQueueToasts}
            onChange={(value) =>
              updateDraft((current) => ({
                ...current,
                autoExpandActiveQueueToasts: value,
              }))
            }
          />
        }
      />
      <SettingRow
        icon={<HistoryIcon className={iconClass} />}
        title="Restore last session"
        description="Reopen your tabs and folders on launch"
        control={
          <ToggleSwitch
            label="Restore last session"
            checked={draft.layout.restoreSession}
            onChange={(value) => updateLayout('restoreSession', value)}
          />
        }
      />
    </>
  )
}

function AutoFolderSizeSetting({ draft, updateDraft }: SectionProps) {
  const folderSizeStatus = usePanesStore((state) => state.folderSizeIndexStatus)
  const showsAutoFolderSize =
    folderSizeStatus?.isAvailable === true && folderSizeStatus.backend !== 'none'
  if (!showsAutoFolderSize) {
    return null
  }
  return (
    <SettingRow
      icon={<PieChartIcon className={iconClass} />}
      title="Automatically calculate folder sizes"
      description={
        folderSizeStatus.backend === 'macIndex' ? (
          <>
            Sizes folders in the background as you browse. Folders you open are sized right away.
            <FolderSizeStatusLine status={folderSizeStatus} />
          </>
        ) : (
          'Uses the Everything index to size folders while browsing. Only available on Windows with Everything installed and indexed.'
        )
      }
      control={
        <ToggleSwitch
          label="Automatically calculate folder sizes"
          checked={draft.autoFolderSize}
          onChange={(value) => updateDraft((current) => ({ ...current, autoFolderSize: value }))}
        />
      }
    />
  )
}
