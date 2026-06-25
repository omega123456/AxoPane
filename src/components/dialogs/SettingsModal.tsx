import { useMemo, useState } from 'react'
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
  SettingRow,
  ToggleSwitch,
} from '@/components/controls'
import { persistAppConfig } from '@/lib/app-config'
import { columnDefinitions } from '@/lib/columns'
import {
  captureShortcut,
  commandLabels,
  defaultKeymap,
  detectPlatformOs,
  findKeybindingConflicts,
  formatShortcutLabel,
  mergeKeymap,
} from '@/lib/keymap'
import type { ColumnConfig, CommandId, LayoutConfig, Shortcut, ThemePreference } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { defaultColumns, defaultLayout, useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useThemeStore } from '@/stores/theme-store'

type Section = 'layout' | 'columns' | 'keybindings'

const sectionNav: { key: Section; label: string }[] = [
  { key: 'layout', label: 'View & Layout' },
  { key: 'columns', label: 'Columns' },
  { key: 'keybindings', label: 'Keybindings' },
]

type DraftState = {
  bindings: Record<CommandId, Shortcut[]>
  columns: ColumnConfig[]
  layout: LayoutConfig
  theme: ThemePreference
  showHiddenFiles: boolean
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
      treeWidth: layout.treeWidth,
      defaultPaneMode: layout.defaultPaneMode,
      restoreSession: layout.restoreSession,
    },
    theme: config.theme,
    showHiddenFiles: config.showHiddenFiles,
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
  const isWindows = os === 'windows'
  const [draft, setDraft] = useState<DraftState>(() => cloneDraft())
  const [search, setSearch] = useState('')
  const [capturing, setCapturing] = useState<CommandId | null>(null)

  const conflicts = useMemo(() => {
    const lookup = new Map<CommandId, string[]>()
    for (const [shortcut, commandIds] of findKeybindingConflicts(draft.bindings)) {
      for (const commandId of commandIds) {
        lookup.set(commandId, [...(lookup.get(commandId) ?? []), shortcut])
      }
    }
    return lookup
  }, [draft.bindings])

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
      if (!isWindows) {
        void commit(next)
      }
      return next
    })
  }

  function updateLayout<K extends keyof LayoutConfig>(key: K, value: LayoutConfig[K]) {
    updateDraft((current) => ({ ...current, layout: { ...current.layout, [key]: value } }))
  }

  async function onSave() {
    await commit(draft)
    close()
  }

  function onCancel() {
    setDraft(cloneDraft())
    close()
  }

  function onReset() {
    updateDraft(() => ({
      bindings: mergeKeymap({}),
      columns: defaultColumns.map((column) => ({ ...column })),
      layout: { ...defaultLayout },
      theme: 'system',
      showHiddenFiles: false,
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
          <span className="text-usm font-semibold text-light-text dark:text-dark-text">Settings</span>
          <button
            type="button"
            aria-label="Close settings"
            onClick={onCancel}
            className="ml-auto flex size-7 items-center justify-center rounded-md text-light-text-muted hover:bg-accent-red hover:text-white focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-muted"
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
                  className={`relative flex h-8.5 items-center rounded-tab px-3 text-usm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
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
              File Explorer
              <br />
              build 0.1.0
            </div>
          </nav>

          <div className="min-h-0 flex-1 overflow-auto px-6 py-5">
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
                <SectionLabel className="mb-3 mt-5">Layout</SectionLabel>
                <SettingRow
                  title="Details panel"
                  description="Show the details panel beside the active pane"
                  control={
                    <ToggleSwitch
                      label="Details panel"
                      checked={draft.layout.detailsVisible}
                      onChange={(value) => updateLayout('detailsVisible', value)}
                    />
                  }
                />
                <SettingRow
                  fixedCopy
                  title="Tree width"
                  description="Width of the folder sidebar"
                  control={
                    <SegmentedControl
                      ariaLabel="Tree width"
                      value={draft.layout.treeWidth}
                      onChange={(value) => updateLayout('treeWidth', value)}
                      options={[
                        { value: 'compact', label: 'Compact' },
                        { value: 'default', label: 'Default' },
                        { value: 'wide', label: 'Wide' },
                      ]}
                    />
                  }
                />
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
                      onDragStart={(event) => event.dataTransfer.setData('text/column', column.key)}
                      onDragOver={(event) => event.preventDefault()}
                      onDrop={(event) => {
                        const fromKey = event.dataTransfer.getData('text/column') as ColumnConfig['key']
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
                      className="flex items-center justify-between rounded-tab border border-light-border bg-light-surface px-3 py-2.5 dark:border-dark-border dark:bg-dark-surface"
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
                              item.key === column.key ? { ...item, visible: !item.visible } : item,
                            ),
                          }))
                        }
                      />
                    </li>
                  ))}
                </ul>
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
                    className="w-full bg-transparent text-row text-light-text focus-visible:outline-none dark:text-dark-text"
                  />
                </label>
                <table className="w-full table-fixed">
                  <thead>
                    <tr className="border-b border-light-border text-left text-2xs uppercase tracking-wide text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
                      <th className="pb-2 font-bold">Command</th>
                      <th className="pb-2 font-bold">Shortcut</th>
                      <th className="pb-2 font-bold">Status</th>
                      <th className="pb-2 text-right font-bold">Reset</th>
                    </tr>
                  </thead>
                  <tbody>
                    {filteredCommands.map((commandId) => {
                      const shortcut = draft.bindings[commandId][0] ?? ''
                      const hasConflict = (conflicts.get(commandId)?.length ?? 0) > 0
                      return (
                        <tr key={commandId} className="border-b border-light-border dark:border-dark-border">
                          <td className="py-3 text-row text-light-text dark:text-dark-text">
                            {commandLabels[commandId]}
                          </td>
                          <td className="py-3">
                            <button
                              type="button"
                              aria-label={`Capture ${commandLabels[commandId]} shortcut`}
                              onClick={() => setCapturing(commandId)}
                              onKeyDown={(event) => {
                                if (capturing !== commandId) {
                                  return
                                }

                                event.preventDefault()
                                const shortcutValue = captureShortcut(event.nativeEvent)
                                if (!shortcutValue) {
                                  return
                                }

                                setCapturing(null)
                                updateDraft((current) => ({
                                  ...current,
                                  bindings: {
                                    ...current.bindings,
                                    [commandId]: [shortcutValue],
                                  },
                                }))
                              }}
                              className="w-36 rounded-tab border border-light-border-strong bg-light-surface px-3 py-2 text-left font-mono text-row text-light-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border-strong dark:bg-dark-surface dark:text-dark-text"
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
                              className="rounded-tab px-3 py-2 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-dark-text-soft"
                            >
                              Reset
                            </button>
                          </td>
                        </tr>
                      )
                    })}
                  </tbody>
                </table>
              </div>
            ) : null}
          </div>
        </div>

        <footer className="flex h-modal-footer flex-none items-center gap-2.5 border-t border-light-border bg-light-titlebar px-4 dark:border-dark-border dark:bg-dark-titlebar">
          <Button variant="ghost" onClick={onReset}>
            Reset to defaults
          </Button>
          <div className="flex-1" />
          {isWindows ? (
            <>
              <Button onClick={onCancel}>Cancel</Button>
              <Button variant="primary" onClick={() => void onSave()}>
                Save changes
              </Button>
            </>
          ) : (
            <span className="text-uxs text-light-text-muted dark:text-dark-text-muted">
              Changes apply immediately on macOS.
            </span>
          )}
        </footer>
      </div>
    </div>
  )
}
