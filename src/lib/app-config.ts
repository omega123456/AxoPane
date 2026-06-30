import { saveConfig } from '@/lib/ipc/commands'
import type { AppConfig, LogLevel, ThemePreference } from '@/lib/types/ipc'
import {
  DEFAULT_UPDATE_INTERVAL,
  isUpdateInterval,
  type UpdateInterval,
} from '@/lib/update-intervals'
import { useConfigStore } from '@/stores/config-store'
import { defaultColumns, defaultLayout, useLayoutStore } from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { migrateKeymap } from '@/lib/keymap'

export function defaultAppConfig(): AppConfig {
  return {
    theme: 'system',
    showHiddenFiles: false,
    dismissedEverythingBanner: false,
    keybindings: {},
    columns: defaultColumns,
    layout: defaultLayout,
    updateCheckInterval: DEFAULT_UPDATE_INTERVAL,
    logLevel: 'info',
  }
}

const LOG_LEVELS: readonly LogLevel[] = ['error', 'warn', 'info', 'debug', 'trace']

export function isLogLevel(value: string): value is LogLevel {
  return (LOG_LEVELS as readonly string[]).includes(value)
}

export function buildAppConfig(): AppConfig {
  const config = useConfigStore.getState()
  const layout = useLayoutStore.getState()
  const keymap = useKeymapStore.getState()

  return {
    theme: config.theme,
    showHiddenFiles: config.showHiddenFiles,
    dismissedEverythingBanner: config.dismissedEverythingBanner,
    keybindings: keymap.bindings,
    columns: layout.columns,
    layout: {
      detailsVisible: false,
      treeWidthPx: layout.treeWidthPx,
      paneSplit: layout.paneSplit,
      columnWidths: layout.columnWidths,
      defaultPaneMode: layout.defaultPaneMode,
      restoreSession: layout.restoreSession,
      zoom: layout.zoom,
    },
    updateCheckInterval: config.updateCheckInterval,
    logLevel: config.logLevel,
  }
}

export function hydrateAppConfig(config: AppConfig) {
  const { bindings, migrated } = migrateKeymap(config.keybindings)
  const updateCheckInterval: UpdateInterval = isUpdateInterval(config.updateCheckInterval ?? '')
    ? config.updateCheckInterval
    : DEFAULT_UPDATE_INTERVAL
  const logLevel: LogLevel = isLogLevel(config.logLevel ?? '') ? config.logLevel : 'info'
  const next = {
    ...defaultAppConfig(),
    ...config,
    keybindings: bindings,
    layout: { ...defaultLayout, ...config.layout, detailsVisible: false },
    columns: config.columns?.length ? config.columns : defaultColumns,
    updateCheckInterval,
    logLevel,
  }

  useConfigStore.getState().hydrate({
    theme: next.theme as ThemePreference,
    showHiddenFiles: next.showHiddenFiles,
    dismissedEverythingBanner: next.dismissedEverythingBanner,
    updateCheckInterval: next.updateCheckInterval,
    logLevel: next.logLevel,
  })
  useLayoutStore.getState().hydrate(next.layout, next.columns)
  useKeymapStore.getState().hydrate(next.keybindings)

  return {
    config: next,
    migrated,
  }
}

export async function persistAppConfig() {
  const config = buildAppConfig()
  await saveConfig(config)
  return config
}
