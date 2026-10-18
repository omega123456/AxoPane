import { useEffect, useEffectEvent, useMemo, useState } from 'react'
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
import { DEFAULT_UPDATE_INTERVAL, type UpdateInterval } from '@/lib/update-intervals'
import { log } from '@/lib/app-log-commands'
import { getAppVersion } from '@/lib/updater'
import {
  captureShortcut,
  commandLabels,
  defaultKeymap,
  detectPlatformOs,
  findKeybindingConflicts,
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
  ThemePreference,
} from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { defaultColumns, defaultLayout, useLayoutStore, zoomLevels } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useThemeStore } from '@/stores/theme-store'
import { FOCUS_RING, FOCUS_RING_INSET } from '@/lib/focus-ring'

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

type DraftState = {
  bindings: Record<CommandId, Shortcut[]>
  columns: ColumnConfig[]
  layout: LayoutConfig
  theme: ThemePreference
  showHiddenFiles: boolean
  updateCheckInterval: UpdateInterval
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
  relativeDates: boolean
  autoFolderSize: boolean
  autoExpandActiveQueueToasts: boolean
}

function cloneDraft(): DraftState {
  const layout = useLayoutStore.getState()
  const keymap = useKeymapStore.getState()
  const config = useConfigStore.getState()
  return {
    bindings: mergeKeymap(keymap.bindings),
    columns: layout.columns.map((column) => ({ ...column })),
    layout: {
      detailsVisible: layout.detailsVisible,
      treeWidthPx: layout.treeWidthPx,
      favouritesHeightPx: layout.favouritesHeightPx,
      paneSplit: layout.paneSplit,
      columnWidths: layout.columnWidths,
      defaultPaneMode: layout.defaultPaneMode,
      defaultViewMode: layout.defaultViewMode,
      restoreSession: layout.restoreSession,
      zoom: layout.zoom,
    },
    theme: config.theme,
    showHiddenFiles: config.showHiddenFiles,
    updateCheckInterval: config.updateCheckInterval,
    dateFormat: config.dateFormat,
    showTime: config.showTime,
    showSeconds: config.showSeconds,
    relativeDates: config.relativeDates,
    autoFolderSize: config.autoFolderSize,
    autoExpandActiveQueueToasts: config.autoExpandActiveQueueToasts,
  }
}

function applyDraft(draft: DraftState) {
  const config = useConfigStore.getState()
  useKeymapStore.getState().hydrate(draft.bindings)
  useLayoutStore.getState().hydrate(draft.layout, draft.columns)
  useConfigStore.setState({
    ...config,
    theme: draft.theme,
    showHiddenFiles: draft.showHiddenFiles,
    updateCheckInterval: draft.updateCheckInterval,
    dateFormat: draft.dateFormat,
    showTime: draft.showTime,
    showSeconds: draft.showSeconds,
    relativeDates: draft.relativeDates,
    autoFolderSize: draft.autoFolderSize,
    autoExpandActiveQueueToasts: draft.autoExpandActiveQueueToasts,
  })
  useThemeStore.getState().setThemePreference(draft.theme)
  usePanesStore.setState({ showHiddenFiles: draft.showHiddenFiles })
}

const compactCount = new Intl.NumberFormat('en', {
  notation: 'compact',
  maximumFractionDigits: 1,
})

function folderSizeStatusLabel(status: FolderSizeIndexStatus) {
  switch (status.state) {
    case 'upToDate':
      return 'Up to date'
    case 'paused':
      return status.pauseReason === 'thermal'
        ? 'Paused to keep your Mac cool'
        : 'Paused in Low Power Mode'
    case 'indexing':
      return 'Indexing…'
    case 'updating':
      return 'Updating…'
    default:
      return 'Preparing…'
  }
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
  const os = detectPlatformOs()
  const folderSizeStatus = usePanesStore((state) => state.folderSizeIndexStatus)
  const logLevel = useConfigStore((state) => state.logLevel)
  const setLogLevel = useConfigStore((state) => state.setLogLevel)
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

  const filteredCommands = (Object.keys(commandLabels) as CommandId[]).filter((commandId) =>
    commandLabels[commandId].toLowerCase().includes(search.toLowerCase()),
  )

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

          <div className="min-h-0 flex-1 overflow-auto pb-7 pl-3 pr-7 pt-1 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-tertiary dark:scrollbar-thumb-dark-text-tertiary">
            <div className="mx-auto grid w-full max-w-settings-content content-start gap-1">
              <h2 className="mb-3.5 font-display text-title font-semibold">
                {sectionNav.find((item) => item.key === section)?.label}
              </h2>
              {section === 'layout' ? (
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
                        onChange={(value) =>
                          updateDraft((current) => ({ ...current, showHiddenFiles: value }))
                        }
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
                  {folderSizeStatus?.isAvailable && folderSizeStatus.backend !== 'none' ? (
                    <SettingRow
                      icon={<PieChartIcon className={iconClass} />}
                      title="Automatically calculate folder sizes"
                      description={
                        folderSizeStatus.backend === 'macIndex' ? (
                          <>
                            Sizes folders in the background as you browse. Folders you open are
                            sized right away.
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
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}

              {section === 'columns' ? (
                <>
                  <ul className="grid gap-1">
                    {draft.columns.map((column) => (
                      <li
                        key={column.key}
                        draggable
                        onDragStart={(event) =>
                          event.dataTransfer.setData('text/column', column.key)
                        }
                        onDragOver={(event) => event.preventDefault()}
                        onDrop={(event) => {
                          event.preventDefault()
                          const fromKey = event.dataTransfer.getData(
                            'text/column',
                          ) as ColumnConfig['key']
                          updateDraft((current) => {
                            const columns = [...current.columns]
                            const fromIndex = columns.findIndex((item) => item.key === fromKey)
                            const toIndex = columns.findIndex((item) => item.key === column.key)
                            if (fromIndex === -1 || toIndex === -1) {
                              return current
                            }
                            const [moved] = columns.splice(fromIndex, 1)
                            columns.splice(toIndex, 0, moved)
                            return { ...current, columns }
                          })
                        }}
                        className="flex min-h-12 cursor-grab items-center justify-between gap-4 rounded-card border border-light-stroke bg-light-card px-4 py-2 dark:border-dark-stroke dark:bg-dark-card"
                      >
                        <span className="inline-flex items-center gap-4 text-body">
                          <GripVerticalIcon className="size-4 text-light-text-secondary dark:text-dark-text-secondary" />
                          {columnDefinitions[column.key].label}
                        </span>
                        <ToggleSwitch
                          label={`${columnDefinitions[column.key].label} column`}
                          checked={column.visible}
                          onChange={() =>
                            updateDraft((current) => ({
                              ...current,
                              columns: current.columns.map((item) =>
                                item.key === column.key
                                  ? { ...item, visible: !item.visible }
                                  : item,
                              ),
                            }))
                          }
                        />
                      </li>
                    ))}
                  </ul>
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}

              {section === 'dates' ? (
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
                        onChange={(value) =>
                          updateDraft((current) => ({ ...current, dateFormat: value }))
                        }
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
                        onChange={(value) =>
                          updateDraft((current) => ({ ...current, showTime: value }))
                        }
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
                        onChange={(value) =>
                          updateDraft((current) => ({ ...current, showSeconds: value }))
                        }
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
                        onChange={(value) =>
                          updateDraft((current) => ({ ...current, relativeDates: value }))
                        }
                      />
                    }
                  />
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}

              {section === 'keybindings' ? (
                <>
                  <label className="mb-3 flex h-8 items-center gap-2 rounded-control border border-light-control-stroke border-b-light-boundary bg-light-control px-2.5 text-light-text-tertiary focus-within:border-b-2 focus-within:border-b-light-accent focus-within:bg-light-card dark:border-dark-control-stroke dark:border-b-dark-boundary dark:bg-dark-control dark:text-dark-text-tertiary dark:focus-within:border-b-dark-accent dark:focus-within:bg-dark-card">
                    <SearchIcon className={iconClass} />
                    <input
                      aria-label="Search keybindings"
                      placeholder="Search keybindings"
                      autoComplete="off"
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      className="w-full select-text bg-transparent text-row text-light-text outline-none placeholder:text-light-text-tertiary dark:text-dark-text dark:placeholder:text-dark-text-tertiary"
                    />
                  </label>
                  <div className="overflow-hidden rounded-card border border-light-stroke bg-light-card dark:border-dark-stroke dark:bg-dark-card">
                    <table className="w-full table-fixed">
                      <thead>
                        <tr className="border-b border-light-divider text-left text-caption text-light-text-secondary dark:border-dark-divider dark:text-dark-text-secondary">
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
                                    <span className="inline-flex h-8 w-36 items-center px-3 font-mono text-light-text-secondary dark:text-dark-text-secondary">
                                      {formatShortcutLabel(shortcut, os)}
                                    </span>
                                  </td>
                                  <td className="py-1.5 text-light-text-secondary dark:text-dark-text-secondary">
                                    System default
                                  </td>
                                  <td className="py-1.5 pr-4" />
                                </>
                              ) : (
                                <>
                                  <td className="py-1.5">
                                    <Button
                                      aria-label={`Capture ${commandLabels[commandId]} shortcut`}
                                      onClick={() => setCapturing(commandId)}
                                      className="w-36 font-mono"
                                    >
                                      {capturing === commandId
                                        ? 'Press keys...'
                                        : shortcut
                                          ? formatShortcutLabel(shortcut, os)
                                          : 'Unassigned'}
                                    </Button>
                                  </td>
                                  <td className="py-1.5 text-light-text-secondary dark:text-dark-text-secondary">
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
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}

              {section === 'updates' ? (
                <>
                  <UpdatesSettings
                    value={draft.updateCheckInterval}
                    onChange={(value) =>
                      updateDraft((current) => ({ ...current, updateCheckInterval: value }))
                    }
                  />
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}

              {section === 'logs' ? (
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
                  <ResetExpander onReset={onReset} />
                </>
              ) : null}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
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
          <span className="text-caption text-light-text-secondary dark:text-dark-text-secondary">
            Restore every setting to how AxoPane ships
          </span>
        </span>
        <ChevronDownIcon
          className={`size-3 text-light-text-secondary dark:text-dark-text-secondary ${expanded ? 'rotate-180' : ''}`}
        />
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
