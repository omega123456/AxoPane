import { useEffect, useEffectEvent, useMemo, useState } from 'react'
import {
  AlertTriangleIcon,
  GripVerticalIcon,
  SearchIcon,
  SettingsIcon,
  XIcon,
} from '@/components/icons'
import {
  Button,
  SectionLabel,
  SegmentedControl,
  SelectField,
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

type Section = 'layout' | 'columns' | 'keybindings' | 'dates' | 'updates' | 'logs'

const sectionNav: { key: Section; label: string }[] = [
  { key: 'layout', label: 'View & Layout' },
  { key: 'columns', label: 'Columns' },
  { key: 'dates', label: 'Dates' },
  { key: 'keybindings', label: 'Keybindings' },
  { key: 'updates', label: 'Updates' },
  { key: 'logs', label: 'Logs' },
]

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
      paneSplit: layout.paneSplit,
      columnWidths: layout.columnWidths,
      defaultPaneMode: layout.defaultPaneMode,
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
  })
  useThemeStore.getState().setThemePreference(draft.theme)
  usePanesStore.setState({ showHiddenFiles: draft.showHiddenFiles })
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
    }))
  }

  const filteredCommands = (Object.keys(commandLabels) as CommandId[]).filter((commandId) =>
    commandLabels[commandId].toLowerCase().includes(search.toLowerCase()),
  )

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-black/50 p-10">
      <div className="flex h-settings-modal-h max-h-full w-settings-modal-w max-w-full flex-col overflow-hidden rounded-modal border border-light-border-strong bg-light-window shadow-window dark:border-dark-border-strong dark:bg-dark-window">
        <header className="flex h-12 flex-none items-center gap-2.5 border-b border-light-border bg-light-titlebar pl-4 pr-2 dark:border-dark-border dark:bg-dark-titlebar">
          <SettingsIcon className="size-4 text-accent-blue-light dark:text-accent-blue" />
          <span className="text-usm font-semibold text-light-text dark:text-dark-text">
            Settings
          </span>
          <button
            type="button"
            aria-label="Close settings"
            onClick={onCancel}
            className="ml-auto flex size-7 cursor-pointer items-center justify-center rounded-md text-light-text-muted hover:bg-accent-red hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-muted"
          >
            <XIcon className="size-4" />
          </button>
        </header>

        <div className="flex min-h-0 flex-1">
          <nav
            aria-label="Settings sections"
            className="flex w-settings-nav flex-none flex-col gap-0.5 border-r border-light-border bg-light-panel p-2 dark:border-dark-border dark:bg-dark-panel"
          >
            {sectionNav.map((item) => {
              const active = item.key === section
              return (
                <button
                  key={item.key}
                  type="button"
                  onClick={() => open(item.key)}
                  className={`relative flex h-8.5 cursor-pointer items-center rounded-tab px-3 text-usm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
                    active
                      ? 'bg-accent-blue-soft font-semibold text-accent-blue-light dark:text-accent-blue'
                      : 'text-light-text-soft hover:bg-light-hover hover:text-light-text dark:text-dark-text-soft dark:hover:bg-dark-hover dark:hover:text-dark-text'
                  }`}
                >
                  {active ? (
                    <span className="absolute inset-y-1.5 left-0 w-0.5 rounded-full bg-accent-blue-light dark:bg-accent-blue" />
                  ) : null}
                  {item.label}
                </button>
              )
            })}
            <div className="mt-auto px-3 py-2.5 font-mono text-2xs leading-relaxed text-light-text-faint dark:text-dark-text-faint">
              AxoPane
              <br />
              build {appVersion}
            </div>
          </nav>

          <div className="min-h-0 flex-1 overflow-auto px-8 py-6 scrollbar-thin scrollbar-track-transparent scrollbar-thumb-light-text-faint dark:scrollbar-thumb-dark-text-faint">
            <div className="mx-auto w-full max-w-settings-content">
              {section === 'layout' ? (
                <div>
                  <SectionLabel className="mb-3">View</SectionLabel>
                  <SettingRow
                    title="Show hidden files"
                    description="Include hidden and system items in directory listings"
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
                    fixedCopy
                    title="Theme"
                    description="Choose how the explorer matches your desktop appearance"
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
                    fixedCopy
                    title="Zoom"
                    description="Scale the entire interface up or down"
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
                  <SectionLabel className="mb-3 mt-5">Layout</SectionLabel>
                  <SettingRow
                    fixedCopy
                    title="Default pane mode"
                    description="How panes open on a fresh window"
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
                    title="Restore last session"
                    description="Reopen the previous tabs and folders on launch"
                    control={
                      <ToggleSwitch
                        label="Restore last session"
                        checked={draft.layout.restoreSession}
                        onChange={(value) => updateLayout('restoreSession', value)}
                      />
                    }
                  />
                </div>
              ) : null}

              {section === 'columns' ? (
                <div>
                  <SectionLabel className="mb-3">Columns</SectionLabel>
                  <ul className="space-y-1">
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
                        className="flex cursor-grab items-center justify-between rounded-tab border border-light-border bg-light-surface px-3 py-2.5 dark:border-dark-border dark:bg-dark-surface"
                      >
                        <span className="inline-flex items-center gap-3 text-row text-light-text dark:text-dark-text">
                          <GripVerticalIcon className="size-4 text-light-text-muted dark:text-dark-text-muted" />
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
                </div>
              ) : null}

              {section === 'dates' ? (
                <div>
                  <SectionLabel className="mb-3">Date display</SectionLabel>
                  <SettingRow
                    fixedCopy
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
                </div>
              ) : null}

              {section === 'keybindings' ? (
                <div>
                  <SectionLabel className="mb-3">Shortcuts</SectionLabel>
                  <label className="mb-4 flex items-center gap-2 rounded-tab border border-light-border-strong bg-light-surface px-3 py-2 dark:border-dark-border-strong dark:bg-dark-surface">
                    <SearchIcon className="size-4 text-light-text-muted dark:text-dark-text-muted" />
                    <input
                      aria-label="Search keybindings"
                      value={search}
                      onChange={(event) => setSearch(event.target.value)}
                      className="w-full select-text bg-transparent text-row text-light-text focus-visible:outline-none dark:text-dark-text"
                    />
                  </label>
                  <table className="w-full table-fixed">
                    <thead>
                      <tr className="border-b border-light-border text-left text-2xs uppercase tracking-wide text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
                        <th className="pb-2 font-bold">Command</th>
                        <th className="w-44 pb-2 font-bold">Shortcut</th>
                        <th className="w-28 pb-2 font-bold">Status</th>
                        <th className="w-20 pb-2 text-right font-bold">Reset</th>
                      </tr>
                    </thead>
                    <tbody>
                      {filteredCommands.map((commandId) => {
                        const shortcut = draft.bindings[commandId][0] ?? ''
                        const hasConflict = (conflicts.get(commandId)?.length ?? 0) > 0
                        const reserved = isReservedCommand(commandId)
                        return (
                          <tr
                            key={commandId}
                            className="border-b border-light-border dark:border-dark-border"
                          >
                            <td className="py-3 text-row text-light-text dark:text-dark-text">
                              {commandLabels[commandId]}
                            </td>
                            {reserved ? (
                              <>
                                <td className="py-3">
                                  <span className="inline-block w-36 rounded-tab border border-light-border bg-light-panel px-3 py-2 text-left font-mono text-row text-light-text-muted dark:border-dark-border dark:bg-dark-panel dark:text-dark-text-muted">
                                    {formatShortcutLabel(shortcut, os)}
                                  </span>
                                </td>
                                <td className="py-3 text-row text-light-text-muted dark:text-dark-text-muted">
                                  System default
                                </td>
                                <td className="py-3" />
                              </>
                            ) : (
                              <>
                                <td className="py-3">
                                  <button
                                    type="button"
                                    aria-label={`Capture ${commandLabels[commandId]} shortcut`}
                                    onClick={() => setCapturing(commandId)}
                                    className="w-36 cursor-pointer rounded-tab border border-light-border-strong bg-light-surface px-3 py-2 text-left font-mono text-row text-light-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border-strong dark:bg-dark-surface dark:text-dark-text"
                                  >
                                    {capturing === commandId
                                      ? 'Press keys...'
                                      : shortcut
                                        ? formatShortcutLabel(shortcut, os)
                                        : 'Unassigned'}
                                  </button>
                                </td>
                                <td className="py-3 text-row text-light-text-muted dark:text-dark-text-muted">
                                  {hasConflict ? (
                                    <span className="inline-flex items-center gap-2 text-accent-amber">
                                      <AlertTriangleIcon className="size-4" />
                                      Conflict
                                    </span>
                                  ) : (
                                    'OK'
                                  )}
                                </td>
                                <td className="py-3 text-right">
                                  <button
                                    type="button"
                                    onClick={() =>
                                      updateDraft((current) => ({
                                        ...current,
                                        bindings: {
                                          ...current.bindings,
                                          [commandId]: [...defaultKeymap[commandId]],
                                        },
                                      }))
                                    }
                                    className="cursor-pointer rounded-tab px-3 py-2 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft"
                                  >
                                    Reset
                                  </button>
                                </td>
                              </>
                            )}
                          </tr>
                        )
                      })}
                    </tbody>
                  </table>
                </div>
              ) : null}

              {section === 'updates' ? (
                <UpdatesSettings
                  value={draft.updateCheckInterval}
                  onChange={(value) =>
                    updateDraft((current) => ({ ...current, updateCheckInterval: value }))
                  }
                />
              ) : null}

              {section === 'logs' ? (
                <div>
                  <SectionLabel className="mb-3">Logging</SectionLabel>
                  <SettingRow
                    fixedCopy
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
                </div>
              ) : null}
            </div>
          </div>
        </div>

        <footer className="flex h-modal-footer flex-none items-center gap-2.5 border-t border-light-border bg-light-titlebar px-4 dark:border-dark-border dark:bg-dark-titlebar">
          <Button variant="ghost" onClick={onReset}>
            Reset to defaults
          </Button>
          <div className="flex-1" />
          <span className="text-uxs text-light-text-muted dark:text-dark-text-muted">
            Changes apply immediately.
          </span>
        </footer>
      </div>
    </div>
  )
}
