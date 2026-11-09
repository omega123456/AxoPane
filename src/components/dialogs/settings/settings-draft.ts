import { persistAppConfig } from '@/lib/app-config'
import { DEFAULT_DATE_FORMAT } from '@/lib/date-format'
import { DEFAULT_UPDATE_INTERVAL } from '@/lib/update-intervals'
import { findKeybindingConflicts, mergeKeymap } from '@/lib/keymap'
import type { ColumnConfig, CommandId, LayoutConfig, Shortcut } from '@/lib/types/ipc'
import { type ConfigSnapshot, useConfigStore } from '@/stores/config-store'
import {
  defaultColumns,
  defaultLayout,
  pickLayoutConfig,
  useLayoutStore,
} from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { usePanesStore } from '@/stores/panes-store'
import { useThemeStore } from '@/stores/theme-store'

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
  'indexFolders',
  'indexFiles',
] as const satisfies readonly (keyof ConfigSnapshot)[]

type DraftConfig = Pick<ConfigSnapshot, (typeof DRAFT_CONFIG_FIELDS)[number]>

export type DraftState = DraftConfig & {
  bindings: Record<CommandId, Shortcut[]>
  columns: ColumnConfig[]
  layout: LayoutConfig
}

export type DraftUpdater = (updater: (current: DraftState) => DraftState) => void

function pickDraftConfig(config: DraftConfig): DraftConfig {
  return Object.fromEntries(
    DRAFT_CONFIG_FIELDS.map((field) => [field, config[field]]),
  ) as DraftConfig
}

export function cloneDraft(): DraftState {
  const layout = useLayoutStore.getState()
  const keymap = useKeymapStore.getState()
  return {
    ...pickDraftConfig(useConfigStore.getState()),
    bindings: mergeKeymap(keymap.bindings),
    columns: layout.columns.map((column) => ({ ...column })),
    layout: pickLayoutConfig(layout),
  }
}

export function defaultDraft(): DraftState {
  return {
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
    indexFolders: false,
    indexFiles: false,
  }
}

export function defaultDraftKeepingIndex(current: DraftState): DraftState {
  return {
    ...defaultDraft(),
    indexFolders: current.indexFolders,
    indexFiles: current.indexFiles,
  }
}

export function withFolderIndex(draft: DraftState, enabled: boolean): DraftState {
  return {
    ...draft,
    indexFolders: enabled,
    indexFiles: enabled && draft.indexFiles,
  }
}

export function withFileIndex(draft: DraftState, enabled: boolean): DraftState {
  return {
    ...draft,
    indexFolders: enabled || draft.indexFolders,
    indexFiles: enabled,
  }
}

function applyDraft(draft: DraftState) {
  useKeymapStore.getState().hydrate(draft.bindings)
  useLayoutStore.getState().hydrate(draft.layout, draft.columns)
  useConfigStore.setState({ ...useConfigStore.getState(), ...pickDraftConfig(draft) })
  useThemeStore.getState().setThemePreference(draft.theme)
  usePanesStore.setState({ showHiddenFiles: draft.showHiddenFiles })
}

export async function commitDraft(nextDraft: DraftState) {
  const previousShowHiddenFiles = usePanesStore.getState().showHiddenFiles
  applyDraft(nextDraft)
  await persistAppConfig()
  if (previousShowHiddenFiles !== nextDraft.showHiddenFiles) {
    const { reloadPane } = usePanesStore.getState()
    await Promise.all([reloadPane('left'), reloadPane('right')])
  }
}

export function shortcutConflictsByCommand(bindings: Record<CommandId, Shortcut[]>) {
  const lookup = new Map<CommandId, string[]>()
  for (const [shortcut, commandIds] of findKeybindingConflicts(bindings)) {
    for (const commandId of commandIds) {
      lookup.set(commandId, [...(lookup.get(commandId) ?? []), shortcut])
    }
  }
  return lookup
}
