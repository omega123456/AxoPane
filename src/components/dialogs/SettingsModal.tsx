import { useEffect, useEffectEvent, useMemo, useState, type ReactNode } from 'react'
import { compactCount, folderSizeStatusLabel } from '@/lib/folder-size-status'
import {
  AlertTriangleIcon,
  CalendarClockIcon,
  CalendarIcon,
  CheckCircleIcon,
  ChevronDownIcon,
  ClockIcon,
  Columns2Icon,
  ContrastIcon,
  DownloadIcon,
  EyeOffIcon,
  FileTextIcon,
  GripVerticalIcon,
  HistoryIcon,
  KeyboardIcon,
  LayersIcon,
  LayoutGridIcon,
  LayoutListIcon,
  ListFilterIcon,
  LoaderCircleIcon,
  PanelsTopLeftIcon,
  PauseIcon,
  PieChartIcon,
  RotateCcwIcon,
  SearchIcon,
  SettingsIcon,
  TimerIcon,
  XIcon,
  ZoomInIcon,
} from '@/components/icons'
import {
  Button,
  SectionLabel,
  SegmentedControl,
  SelectField,
  SelectionPill,
  SettingRow,
  ToggleSwitch,
} from '@/components/controls'
import { UpdatesSettings } from '@/components/dialogs/UpdatesSettings'
import { LogViewer } from '@/components/dialogs/LogViewer'
import { persistAppConfig } from '@/lib/app-config'
import { columnDefinitions } from '@/lib/columns'
import {
  DATE_FORMATS,
  type DateFormat,
  dateFormatLabels,
  DEFAULT_DATE_FORMAT,
} from '@/lib/date-format'
import { DEFAULT_UPDATE_INTERVAL } from '@/lib/update-intervals'
import { log } from '@/lib/app-log-commands'
import { getAppVersion } from '@/lib/updater'
import {
  captureShortcut,
  commandLabels,
  defaultKeymap,
  detectPlatformOs,
  findKeybindingConflicts,
  type PlatformOs,
  formatShortcutLabel,
  isReservedCommand,
  mergeKeymap,
} from '@/lib/keymap'
import type {
  ColumnConfig,
  CommandId,
  FolderSizeIndexStatus,
  LayoutConfig,
  LogLevel,
  Shortcut,
} from '@/lib/types/ipc'
import { type ConfigSnapshot, useConfigStore } from '@/stores/config-store'
import {
  defaultColumns,
  defaultLayout,
  pickLayoutConfig,
  useLayoutStore,
  zoomLevels,
} from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useThemeStore } from '@/stores/theme-store'
import { FOCUS_RING, FOCUS_RING_INSET } from '@/lib/focus-ring'
import { moveItem } from '@/lib/collections'
import { SCROLLBAR_THIN } from '@/lib/class-names'
import { TEXT_SECONDARY } from '@/lib/class-names'

type Section = 'layout' | 'columns' | 'keybindings' | 'dates' | 'updates' | 'logs'

const sectionNav: { key: Section; label: string; Icon: typeof SettingsIcon }[] = [
  { key: 'layout', label: 'View and layout', Icon: LayoutListIcon },
  { key: 'columns', label: 'Columns', Icon: PanelsTopLeftIcon },
  { key: 'dates', label: 'Dates', Icon: CalendarIcon },
  { key: 'keybindings', label: 'Keybindings', Icon: KeyboardIcon },
  { key: 'updates', label: 'Updates', Icon: DownloadIcon },
  { key: 'logs', label: 'Logs', Icon: FileTextIcon },
]

const iconClass = 'size-4'

const DATE_FORMAT_OPTIONS: { value: DateFormat; label: string }[] = DATE_FORMATS.map((format) => ({
  value: format,
  label: dateFormatLabels[format],
}))

const LOG_LEVEL_OPTIONS: { value: LogLevel; label: string }[] = [
  { value: 'error', label: 'Error' },
  { value: 'warn', label: 'Warn' },
  { value: 'info', label: 'Info' },
  { value: 'debug', label: 'Debug' },
  { value: 'trace', label: 'Trace' },
]

const DRAFT_CONFIG_FIELDS = [
  'theme',
  'showHiddenFiles',
  'updateCheckInterval',
  'dateFormat',
  'showTime',
  'showSeconds',
  'relativeDates',
  'autoFolderSize',
  'autoExpandActiveQueueToasts',
] as const satisfies readonly (keyof ConfigSnapshot)[]

type DraftConfig = Pick<ConfigSnapshot, (typeof DRAFT_CONFIG_FIELDS)[number]>

type DraftState = DraftConfig & {
  bindings: Record<CommandId, Shortcut[]>
  columns: ColumnConfig[]
  layout: LayoutConfig
}

function pickDraftConfig(config: DraftConfig): DraftConfig {
  return Object.fromEntries(
    DRAFT_CONFIG_FIELDS.map((field) => [field, config[field]]),
  ) as DraftConfig
}

function cloneDraft(): DraftState {
  const layout = useLayoutStore.getState()
  const keymap = useKeymapStore.getState()
  return {
    ...pickDraftConfig(useConfigStore.getState()),
    bindings: mergeKeymap(keymap.bindings),
    columns: layout.columns.map((column) => ({ ...column })),
    layout: pickLayoutConfig(layout),
  }
}

function applyDraft(draft: DraftState) {
  useKeymapStore.getState().hydrate(draft.bindings)
  useLayoutStore.getState().hydrate(draft.layout, draft.columns)
  useConfigStore.setState({ ...useConfigStore.getState(), ...pickDraftConfig(draft) })
  useThemeStore.getState().setThemePreference(draft.theme)
  usePanesStore.setState({ showHiddenFiles: draft.showHiddenFiles })
}

/** The macOS index's state, shown under the auto folder size description. */
function FolderSizeStatusLine({ status }: { status: FolderSizeIndexStatus }) {
  const iconClass = 'h-3.5 w-3.5 shrink-0'
  const label = folderSizeStatusLabel(status)
  const count =
    status.state === 'indexing' && status.itemsScanned !== undefined
      ? ` ${compactCount.format(status.itemsScanned)} items`
      : ''

  // The status region announces state transitions only, not count ticks.
  return (
    <span className="mt-1 flex items-center gap-1">
      {status.state === 'upToDate' ? (
        <CheckCircleIcon className={iconClass} />
      ) : status.state === 'paused' ? (
        <PauseIcon className={iconClass} />
      ) : (
        <LoaderCircleIcon className={`${iconClass} animate-spin`} />
      )}
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

export function SettingsModal() {
  const isOpen = useSettingsStore((state) => state.isOpen)
  if (!isOpen) {
    return null
  }

  return <SettingsModalContent />
}

function SettingsModalContent() {
  const section = useSettingsStore((state) => state.section)
  const close = useSettingsStore((state) => state.close)
  const open = useSettingsStore((state) => state.open)
  const platform = detectPlatformOs()
  const [draft, setDraft] = useState<DraftState>(() => cloneDraft())
  const [search, setSearch] = useState('')
  const [capturing, setCapturing] = useState<CommandId | null>(null)
  const [appVersion, setAppVersion] = useState('…')

  useEffect(() => {
    let cancelled = false
    void getAppVersion()
      .then((version) => {
        if (!cancelled) {
          setAppVersion(version)
        }
      })
      .catch((cause: unknown) => {
        log.error('failed to read app version', { error: String(cause) })
        if (!cancelled) {
          setAppVersion('unknown')
        }
      })
    return () => {
      cancelled = true
    }
  }, [])

  const conflicts = useMemo(() => {
    const lookup = new Map<CommandId, string[]>()
    for (const [shortcut, commandIds] of findKeybindingConflicts(draft.bindings)) {
      for (const commandId of commandIds) {
        lookup.set(commandId, [...(lookup.get(commandId) ?? []), shortcut])
      }
    }
    return lookup
  }, [draft.bindings])

  const onCaptureKeyDown = useEffectEvent((event: KeyboardEvent) => {
    if (!capturing) {
      return
    }

    event.preventDefault()
    event.stopPropagation()

    const shortcutValue = captureShortcut(event)
    if (!shortcutValue) {
      return
    }

    setCapturing(null)
    updateDraft((current) => ({
      ...current,
      bindings: {
        ...current.bindings,
        [capturing]: [shortcutValue],
      },
    }))
  })

  useEffect(() => {
    if (!capturing) {
      return
    }

    window.addEventListener('keydown', onCaptureKeyDown, true)
    return () => window.removeEventListener('keydown', onCaptureKeyDown, true)
  }, [capturing])

  async function commit(nextDraft: DraftState) {
    const previousShowHiddenFiles = usePanesStore.getState().showHiddenFiles
    applyDraft(nextDraft)
    await persistAppConfig()
    if (previousShowHiddenFiles !== nextDraft.showHiddenFiles) {
      const { reloadPane } = usePanesStore.getState()
      await Promise.all([reloadPane('left'), reloadPane('right')])
    }
  }

  function updateDraft(updater: (current: DraftState) => DraftState) {
    setDraft((current) => {
      const next = updater(current)
      void commit(next)
      return next
    })
  }

  function updateLayout<K extends keyof LayoutConfig>(key: K, value: LayoutConfig[K]) {
    updateDraft((current) => ({ ...current, layout: { ...current.layout, [key]: value } }))
  }

  function onCancel() {
    close()
  }

  function onReset() {
    updateDraft(() => ({
      bindings: mergeKeymap({}),
      columns: defaultColumns.map((column) => ({ ...column })),
      layout: { ...defaultLayout },
      theme: 'system',
      showHiddenFiles: false,
      updateCheckInterval: DEFAULT_UPDATE_INTERVAL,
      dateFormat: DEFAULT_DATE_FORMAT,
      showTime: false,
      showSeconds: false,
      relativeDates: false,
      autoFolderSize: true,
      autoExpandActiveQueueToasts: false,
    }))
  }

  const SectionContent = sectionComponents[section]

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-scrim p-10">
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Settings"
        className="flex h-settings-h max-h-full w-settings max-w-full flex-col overflow-hidden rounded-overlay border border-light-window-stroke bg-light-base text-light-text shadow-light-dialog dark:border-dark-window-stroke dark:bg-dark-base dark:text-dark-text dark:shadow-dark-dialog"
      >
        <header className="flex h-11 flex-none items-center gap-2.5 pl-4 pr-1.5 text-caption">
          <SettingsIcon className={iconClass} />
          Settings
          <Button
            variant="subtle"
            aria-label="Close settings"
            onClick={onCancel}
            size="icon"
            className="ml-auto"
          >
            <XIcon className={iconClass} />
          </Button>
        </header>

        <div className="flex min-h-0 flex-1">
          <nav
            aria-label="Settings sections"
            className="flex w-settings-nav flex-none flex-col gap-0.5 px-2 pb-4 pt-1"
          >
            {sectionNav.map(({ key, label, Icon }) => {
              const active = key === section
              return (
                <button
                  key={key}
                  type="button"
                  aria-current={active ? 'page' : undefined}
                  onClick={() => open(key)}
                  className={`relative flex h-8.5 flex-none cursor-pointer items-center gap-3 rounded-control px-3 text-body ${FOCUS_RING_INSET} ${
                    active
                      ? 'bg-light-hover dark:bg-dark-hover'
                      : 'hover:bg-light-hover active:bg-light-press dark:hover:bg-dark-hover dark:active:bg-dark-press'
                  }`}
                >
                  {active ? <SelectionPill /> : null}
                  <Icon className={iconClass} />
                  {label}
                </button>
              )
            })}
            <div className="mt-6 px-3 text-caption text-light-text-tertiary dark:text-dark-text-tertiary">
              AxoPane {appVersion}
            </div>
          </nav>

          <div className={`min-h-0 flex-1 overflow-auto pb-7 pl-3 pr-7 pt-1 ${SCROLLBAR_THIN}`}>
            <div className="mx-auto grid w-full max-w-settings-content content-start gap-1">
              <h2 className="mb-3.5 font-display text-title font-semibold">
                {sectionNav.find((item) => item.key === section)?.label}
              </h2>
              <SectionContent
                draft={draft}
                updateDraft={updateDraft}
                updateLayout={updateLayout}
                search={search}
                onSearchChange={setSearch}
                capturing={capturing}
                onCapture={setCapturing}
                conflicts={conflicts}
                platform={platform}
              />
              <ResetExpander key={section} onReset={onReset} />
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

type DraftUpdater = (updater: (current: DraftState) => DraftState) => void

type SectionProps = {
  draft: DraftState
  updateDraft: DraftUpdater
  updateLayout: <Key extends keyof LayoutConfig>(key: Key, value: LayoutConfig[Key]) => void
  search: string
  onSearchChange: (search: string) => void
  capturing: CommandId | null
  onCapture: (commandId: CommandId) => void
  conflicts: Map<CommandId, string[]>
  platform: PlatformOs
}

function LayoutSection({ draft, updateDraft, updateLayout }: SectionProps) {
  const folderSizeStatus = usePanesStore((state) => state.folderSizeIndexStatus)
  const showsAutoFolderSize =
    folderSizeStatus?.isAvailable === true && folderSizeStatus.backend !== 'none'
  return (
    <>
      <SectionLabel>View</SectionLabel>
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
      <SectionLabel>Layout</SectionLabel>
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
      {showsAutoFolderSize ? (
        <SettingRow
          icon={<PieChartIcon className={iconClass} />}
          title="Automatically calculate folder sizes"
          description={
            folderSizeStatus.backend === 'macIndex' ? (
              <>
                Sizes folders in the background as you browse. Folders you open are sized right
                away.
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
              onChange={(value) =>
                updateDraft((current) => ({ ...current, autoFolderSize: value }))
              }
            />
          }
        />
      ) : null}
    </>
  )
}

function ColumnsSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <ul className="grid gap-1">
        {draft.columns.map((column) => (
          <li
            key={column.key}
            draggable
            onDragStart={(event) => event.dataTransfer.setData('text/column', column.key)}
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
              event.preventDefault()
              const fromKey = event.dataTransfer.getData('text/column') as ColumnConfig['key']
              updateDraft((current) => {
                const { columns } = current
                const fromIndex = columns.findIndex((item) => item.key === fromKey)
                const toIndex = columns.findIndex((item) => item.key === column.key)
                const columnMissing = fromIndex === -1 || toIndex === -1
                if (columnMissing) {
                  return current
                }
                return { ...current, columns: moveItem(columns, fromIndex, toIndex) }
              })
            }}
            className="flex min-h-12 cursor-grab items-center justify-between gap-4 rounded-card border border-light-stroke bg-light-card px-4 py-2 dark:border-dark-stroke dark:bg-dark-card"
          >
            <span className="inline-flex items-center gap-4 text-body">
              <GripVerticalIcon className={`size-4 ${TEXT_SECONDARY}`} />
              {columnDefinitions[column.key].label}
            </span>
            <ToggleSwitch
              label={`${columnDefinitions[column.key].label} column`}
              checked={column.visible}
              onChange={() =>
                updateDraft((current) => ({
                  ...current,
                  columns: current.columns.map((item) =>
                    item.key === column.key ? { ...item, visible: !item.visible } : item,
                  ),
                }))
              }
            />
          </li>
        ))}
      </ul>
    </>
  )
}

function DatesSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <SectionLabel>Date display</SectionLabel>
      <SettingRow
        icon={<CalendarIcon className={iconClass} />}
        title="Date format"
        description="How modified and created dates are written in lists and properties"
        control={
          <SelectField
            ariaLabel="Date format"
            value={draft.dateFormat}
            onChange={(value) => updateDraft((current) => ({ ...current, dateFormat: value }))}
            options={DATE_FORMAT_OPTIONS}
          />
        }
      />
      <SettingRow
        icon={<ClockIcon className={iconClass} />}
        title="Show time"
        description="Append the time of day (HH:MM) to the date"
        control={
          <ToggleSwitch
            label="Show time"
            checked={draft.showTime}
            onChange={(value) => updateDraft((current) => ({ ...current, showTime: value }))}
          />
        }
      />
      <SettingRow
        icon={<TimerIcon className={iconClass} />}
        title="Show seconds"
        description="Include seconds (HH:MM:SS) when the time is shown"
        control={
          <ToggleSwitch
            label="Show seconds"
            checked={draft.showSeconds}
            onChange={(value) => updateDraft((current) => ({ ...current, showSeconds: value }))}
          />
        }
      />
      <SettingRow
        icon={<CalendarClockIcon className={iconClass} />}
        title="Relative dates"
        description="Show colour-coded phrases like “15 minutes ago” for items changed within the last 2 days, then fall back to the format above"
        control={
          <ToggleSwitch
            label="Relative dates"
            checked={draft.relativeDates}
            onChange={(value) => updateDraft((current) => ({ ...current, relativeDates: value }))}
          />
        }
      />
    </>
  )
}

function KeybindingsSection({
  draft,
  updateDraft,
  search,
  onSearchChange,
  capturing,
  onCapture,
  conflicts,
  platform,
}: SectionProps) {
  const filteredCommands = (Object.keys(commandLabels) as CommandId[]).filter((commandId) =>
    commandLabels[commandId].toLowerCase().includes(search.toLowerCase()),
  )
  return (
    <>
      <label className="mb-3 flex h-8 items-center gap-2 rounded-control border border-light-control-stroke border-b-light-boundary bg-light-control px-2.5 text-light-text-tertiary focus-within:border-b-2 focus-within:border-b-light-accent focus-within:bg-light-card dark:border-dark-control-stroke dark:border-b-dark-boundary dark:bg-dark-control dark:text-dark-text-tertiary dark:focus-within:border-b-dark-accent dark:focus-within:bg-dark-card">
        <SearchIcon className={iconClass} />
        <input
          aria-label="Search keybindings"
          placeholder="Search keybindings"
          autoComplete="off"
          value={search}
          onChange={(event) => onSearchChange(event.target.value)}
          className="w-full select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-tertiary dark:text-dark-text dark:placeholder:text-dark-text-tertiary"
        />
      </label>
      <div className="overflow-hidden rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card">
        <table className="w-full table-fixed">
          <thead>
            <tr
              className={`border-b border-light-divider text-left text-caption ${TEXT_SECONDARY} dark:border-dark-divider`}
            >
              <th className="py-2 pl-4 font-normal">Command</th>
              <th className="w-44 py-2 font-normal">Shortcut</th>
              <th className="w-28 py-2 font-normal">Status</th>
              <th className="w-20 py-2 pr-4 text-right font-normal">Reset</th>
            </tr>
          </thead>
          <tbody className="text-row">
            {filteredCommands.map((commandId) => {
              const shortcut = draft.bindings[commandId][0] ?? ''
              const hasConflict = (conflicts.get(commandId)?.length ?? 0) > 0
              const reserved = isReservedCommand(commandId)
              return (
                <tr
                  key={commandId}
                  className="border-b border-light-divider last:border-b-0 dark:border-dark-divider"
                >
                  <td className="py-1.5 pl-4">{commandLabels[commandId]}</td>
                  {reserved ? (
                    <>
                      <td className="py-1.5">
                        <span
                          className={`inline-flex h-8 w-36 items-center px-3 font-mono ${TEXT_SECONDARY}`}
                        >
                          {formatShortcutLabel(shortcut, platform)}
                        </span>
                      </td>
                      <td className={`py-1.5 ${TEXT_SECONDARY}`}>System default</td>
                      <td className="py-1.5 pr-4" />
                    </>
                  ) : (
                    <>
                      <td className="py-1.5">
                        <Button
                          aria-label={`Capture ${commandLabels[commandId]} shortcut`}
                          onClick={() => onCapture(commandId)}
                          className="w-36 font-mono"
                        >
                          {capturing === commandId
                            ? 'Press keys...'
                            : shortcut
                              ? formatShortcutLabel(shortcut, platform)
                              : 'Unassigned'}
                        </Button>
                      </td>
                      <td className={`py-1.5 ${TEXT_SECONDARY}`}>
                        {hasConflict ? (
                          <span className="inline-flex items-center gap-2 text-light-warning dark:text-dark-warning">
                            <AlertTriangleIcon className={iconClass} />
                            Conflict
                          </span>
                        ) : (
                          'OK'
                        )}
                      </td>
                      <td className="py-1.5 pr-4 text-right">
                        <Button
                          variant="subtle"
                          onClick={() =>
                            updateDraft((current) => ({
                              ...current,
                              bindings: {
                                ...current.bindings,
                                [commandId]: [...defaultKeymap[commandId]],
                              },
                            }))
                          }
                        >
                          Reset
                        </Button>
                      </td>
                    </>
                  )}
                </tr>
              )
            })}
          </tbody>
        </table>
      </div>
    </>
  )
}

function UpdatesSection({ draft, updateDraft }: SectionProps) {
  return (
    <>
      <UpdatesSettings
        value={draft.updateCheckInterval}
        onChange={(value) => updateDraft((current) => ({ ...current, updateCheckInterval: value }))}
      />
    </>
  )
}

function LogsSection() {
  const logLevel = useConfigStore((state) => state.logLevel)
  const setLogLevel = useConfigStore((state) => state.setLogLevel)
  return (
    <>
      <SectionLabel>Logging</SectionLabel>
      <SettingRow
        icon={<ListFilterIcon className={iconClass} />}
        title="Capture level"
        description="Minimum severity written to the daily log file"
        control={
          <SelectField
            ariaLabel="Capture level"
            value={logLevel}
            onChange={(value) => void setLogLevel(value)}
            options={LOG_LEVEL_OPTIONS}
          />
        }
      />
      <LogViewer />
    </>
  )
}

const sectionComponents: Record<Section, (props: SectionProps) => ReactNode> = {
  layout: LayoutSection,
  columns: ColumnsSection,
  dates: DatesSection,
  keybindings: KeybindingsSection,
  updates: UpdatesSection,
  logs: LogsSection,
}

/**
 * "Reset to defaults" at the foot of every settings page: a card that expands
 * to confirm, so a stray click can't wipe the configuration.
 */
function ResetExpander({ onReset }: { onReset: () => void }) {
  const [expanded, setExpanded] = useState(false)
  return (
    <div className="mt-4.5 rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card">
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded((value) => !value)}
        className={`flex min-h-16 w-full cursor-pointer items-center gap-4 rounded-card px-4 py-2.5 text-left hover:bg-light-hover dark:hover:bg-dark-hover ${FOCUS_RING}`}
      >
        <RotateCcwIcon className={iconClass} />
        <span className="grid min-w-0 flex-1">
          <span className="text-body">Reset to defaults</span>
          <span className={`text-caption ${TEXT_SECONDARY}`}>
            Restore every setting to how AxoPane ships
          </span>
        </span>
        <ChevronDownIcon className={`size-3 ${TEXT_SECONDARY} ${expanded ? 'rotate-180' : ''}`} />
      </button>
      {expanded ? (
        <div className="flex items-center gap-4 border-t border-light-divider py-3 pl-12 pr-4 dark:border-dark-divider">
          <span className="flex-1 text-body">Settings change right away. You can't undo this.</span>
          <Button onClick={onReset}>Reset</Button>
        </div>
      ) : null}
    </div>
  )
}
