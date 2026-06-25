import { useMemo, useState } from 'react'
import {
  AlertTriangleIcon,
  GripVerticalIcon,
  SearchIcon,
  SquareCheckIcon,
  SquareIcon,
  XIcon,
} from '@/components/icons'
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
import type { ColumnConfig, CommandId, LayoutConfig, Shortcut } from '@/lib/types/ipc'
import { useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useSettingsStore } from '@/stores/settings-store'

type Section = 'keybindings' | 'columns' | 'layout'

type DraftState = {
  bindings: Record<CommandId, Shortcut[]>
  columns: ColumnConfig[]
  layout: LayoutConfig
}

function cloneDraft(): DraftState {
  const layout = useLayoutStore.getState()
  const keymap = useKeymapStore.getState()
  return {
    bindings: mergeKeymap(keymap.bindings),
    columns: layout.columns.map((column) => ({ ...column })),
    layout: {
      detailsVisible: layout.detailsVisible,
      treeWidth: layout.treeWidth,
      defaultPaneMode: layout.defaultPaneMode,
      restoreSession: layout.restoreSession,
    },
  }
}

function applyDraft(draft: DraftState) {
  useKeymapStore.getState().hydrate(draft.bindings)
  useLayoutStore.getState().hydrate(draft.layout, draft.columns)
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
    applyDraft(nextDraft)
    await persistAppConfig()
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

  async function onSave() {
    await commit(draft)
    close()
  }

  function onCancel() {
    setDraft(cloneDraft())
    close()
  }

  const filteredCommands = (Object.keys(commandLabels) as CommandId[]).filter((commandId) =>
    commandLabels[commandId].toLowerCase().includes(search.toLowerCase()),
  )

  return (
    <div className="fixed inset-0 z-40 flex items-center justify-center bg-slate-950/35 px-6 py-10">
      <div className="flex h-full max-h-160 w-full max-w-5xl overflow-hidden rounded-window border border-light-border-strong bg-light-panel shadow-window dark:border-dark-border-strong dark:bg-dark-panel">
        <aside className="flex w-44 shrink-0 flex-col border-r border-light-border dark:border-dark-border">
          <div className="flex items-center justify-between border-b border-light-border px-4 py-3 dark:border-dark-border">
            <span className="font-mono text-uxs uppercase tracking-wide text-light-text-muted dark:text-dark-text-muted">
              Settings
            </span>
            <button
              type="button"
              aria-label="Close settings"
              onClick={onCancel}
              className="rounded-tab p-1 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
            >
              <XIcon className="h-4 w-4" />
            </button>
          </div>
          {(['keybindings', 'columns', 'layout'] as Section[]).map((item) => (
            <button
              key={item}
              type="button"
              onClick={() => open(item)}
              className={`px-4 py-3 text-left text-row capitalize focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-inset focus-visible:ring-accent-blue-border ${
                item === section
                  ? 'bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
                  : 'text-light-text dark:text-dark-text'
              }`}
            >
              {item}
            </button>
          ))}
        </aside>
        <div className="flex min-w-0 flex-1 flex-col">
          {section === 'keybindings' ? (
            <div className="flex min-h-0 flex-1 flex-col">
              <div className="border-b border-light-border px-4 py-3 dark:border-dark-border">
                <label className="flex items-center gap-2 rounded-tab border border-light-border bg-light-surface px-3 py-2 dark:border-dark-border dark:bg-dark-surface">
                  <SearchIcon className="h-4 w-4 text-light-text-muted dark:text-dark-text-muted" />
                  <input
                    aria-label="Search keybindings"
                    value={search}
                    onChange={(event) => setSearch(event.target.value)}
                    className="w-full bg-transparent text-row text-light-text focus-visible:outline-none dark:text-dark-text"
                  />
                </label>
              </div>
              <div className="min-h-0 flex-1 overflow-auto px-4 py-3">
                <table className="w-full table-fixed">
                  <thead>
                    <tr className="border-b border-light-border text-left text-uxs uppercase tracking-wide text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
                      <th className="pb-2">Command</th>
                      <th className="pb-2">Shortcut</th>
                      <th className="pb-2">Status</th>
                      <th className="pb-2 text-right">Reset</th>
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
                              className="w-36 rounded-tab border border-light-border bg-light-surface px-3 py-2 text-left font-mono text-row text-light-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:bg-dark-surface dark:text-dark-text"
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
                              <span className="inline-flex items-center gap-2 text-amber-600 dark:text-amber-400">
                                <AlertTriangleIcon className="h-4 w-4" />
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
            </div>
          ) : null}
          {section === 'columns' ? (
            <div className="min-h-0 flex-1 overflow-auto px-4 py-4">
              <ul className="space-y-2">
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
                    className="flex items-center justify-between rounded-tab border border-light-border bg-light-surface px-3 py-3 dark:border-dark-border dark:bg-dark-surface"
                  >
                    <span className="inline-flex items-center gap-3 text-row text-light-text dark:text-dark-text">
                      <GripVerticalIcon className="h-4 w-4 text-light-text-muted dark:text-dark-text-muted" />
                      {columnDefinitions[column.key].label}
                    </span>
                    <button
                      type="button"
                      onClick={() =>
                        updateDraft((current) => ({
                          ...current,
                          columns: current.columns.map((item) =>
                            item.key === column.key ? { ...item, visible: !item.visible } : item,
                          ),
                        }))
                      }
                      className="inline-flex items-center gap-2 rounded-tab px-3 py-2 text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
                    >
                      {column.visible ? <SquareCheckIcon className="h-4 w-4" /> : <SquareIcon className="h-4 w-4" />}
                      {column.visible ? 'Shown' : 'Hidden'}
                    </button>
                  </li>
                ))}
              </ul>
            </div>
          ) : null}
          {section === 'layout' ? (
            <div className="space-y-6 px-4 py-4">
              <LayoutToggle
                label="Details panel"
                value={draft.layout.detailsVisible}
                onChange={(value) =>
                  updateDraft((current) => ({
                    ...current,
                    layout: { ...current.layout, detailsVisible: value },
                  }))
                }
              />
              <fieldset>
                <legend className="mb-2 text-row font-semibold text-light-text dark:text-dark-text">Tree width</legend>
                <div className="flex gap-2">
                  {(['compact', 'default', 'wide'] as LayoutConfig['treeWidth'][]).map((value) => (
                    <ChoiceButton
                      key={value}
                      active={draft.layout.treeWidth === value}
                      label={value}
                      onClick={() =>
                        updateDraft((current) => ({
                          ...current,
                          layout: { ...current.layout, treeWidth: value },
                        }))
                      }
                    />
                  ))}
                </div>
              </fieldset>
              <fieldset>
                <legend className="mb-2 text-row font-semibold text-light-text dark:text-dark-text">Default pane mode</legend>
                <div className="flex gap-2">
                  {(['dual', 'single'] as LayoutConfig['defaultPaneMode'][]).map((value) => (
                    <ChoiceButton
                      key={value}
                      active={draft.layout.defaultPaneMode === value}
                      label={value}
                      onClick={() =>
                        updateDraft((current) => ({
                          ...current,
                          layout: { ...current.layout, defaultPaneMode: value },
                        }))
                      }
                    />
                  ))}
                </div>
              </fieldset>
              <LayoutToggle
                label="Restore last session"
                value={draft.layout.restoreSession}
                onChange={(value) =>
                  updateDraft((current) => ({
                    ...current,
                    layout: { ...current.layout, restoreSession: value },
                  }))
                }
              />
            </div>
          ) : null}
          {isWindows ? (
            <div className="flex justify-end gap-2 border-t border-light-border px-4 py-3 dark:border-dark-border">
              <button
                type="button"
                onClick={onCancel}
                className="rounded-tab border border-light-border px-4 py-2 text-row text-light-text-soft focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:border-dark-border dark:text-dark-text-soft"
              >
                Cancel
              </button>
              <button
                type="button"
                onClick={() => void onSave()}
                className="rounded-tab bg-accent-blue-soft px-4 py-2 text-row font-semibold text-accent-blue-light focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border dark:text-accent-blue"
              >
                Save
              </button>
            </div>
          ) : (
            <div className="border-t border-light-border px-4 py-3 text-row text-light-text-muted dark:border-dark-border dark:text-dark-text-muted">
              Changes apply immediately on macOS.
            </div>
          )}
        </div>
      </div>
    </div>
  )
}

function ChoiceButton({
  active,
  label,
  onClick,
}: {
  active: boolean
  label: string
  onClick: () => void
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`rounded-tab border px-3 py-2 text-row capitalize focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border ${
        active
          ? 'border-accent-blue-border bg-accent-blue-soft text-accent-blue-light dark:text-accent-blue'
          : 'border-light-border text-light-text dark:border-dark-border dark:text-dark-text'
      }`}
    >
      {label}
    </button>
  )
}

function LayoutToggle({
  label,
  onChange,
  value,
}: {
  label: string
  value: boolean
  onChange: (value: boolean) => void
}) {
  return (
    <label className="flex items-center justify-between rounded-tab border border-light-border bg-light-surface px-3 py-3 dark:border-dark-border dark:bg-dark-surface">
      <span className="text-row text-light-text dark:text-dark-text">{label}</span>
      <button
        type="button"
        onClick={() => onChange(!value)}
        className="rounded-tab px-3 py-2 text-row focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-blue-border"
      >
        {value ? 'On' : 'Off'}
      </button>
    </label>
  )
}
