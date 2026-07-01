import { saveConfig } from '@/lib/ipc/commands'
import type { AppConfig, LogLevel, ThemePreference } from '@/lib/types/ipc'
import { DEFAULT_DATE_FORMAT, type DateFormat, isDateFormat } from '@/lib/date-format'
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
    dateFormat: DEFAULT_DATE_FORMAT,
    showTime: false,
    showSeconds: false,
    relativeDates: false,
    autoFolderSize: true,
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
    dateFormat: config.dateFormat,
    showTime: config.showTime,
    showSeconds: config.showSeconds,
    relativeDates: config.relativeDates,
    autoFolderSize: config.autoFolderSize,
  }
}

// Earlier builds folded the full time (`HH:MM:SS`) into the format key (e.g.
// `ymd_his`). Split those legacy values into a base format plus the standalone
// `showTime`/`showSeconds` flags so the user's preference survives the move to
// separate toggles.
function migrateDateSettings(config: AppConfig): {
  dateFormat: DateFormat
  showTime: boolean
  showSeconds: boolean
} {
  const raw = config.dateFormat ?? ''
  const legacyTime = raw.endsWith('_his')
  const base = legacyTime ? raw.slice(0, -'_his'.length) : raw
  const dateFormat: DateFormat = isDateFormat(base) ? base : DEFAULT_DATE_FORMAT
  const showTime = legacyTime || (config.showTime ?? false)
  const showSeconds = legacyTime || (config.showSeconds ?? false)
  return { dateFormat, showTime, showSeconds }
}

export function hydrateAppConfig(config: AppConfig) {
  const { bindings, migrated } = migrateKeymap(config.keybindings)
  const updateCheckInterval: UpdateInterval = isUpdateInterval(config.updateCheckInterval ?? '')
    ? config.updateCheckInterval
    : DEFAULT_UPDATE_INTERVAL
  const logLevel: LogLevel = isLogLevel(config.logLevel ?? '') ? config.logLevel : 'info'
  const { dateFormat, showTime, showSeconds } = migrateDateSettings(config)
  const next = {
    ...defaultAppConfig(),
    ...config,
    keybindings: bindings,
    layout: { ...defaultLayout, ...config.layout, detailsVisible: false },
    columns: config.columns?.length ? config.columns : defaultColumns,
    updateCheckInterval,
    logLevel,
    dateFormat,
    showTime,
    showSeconds,
    relativeDates: config.relativeDates ?? false,
  }

  useConfigStore.getState().hydrate({
    theme: next.theme as ThemePreference,
    showHiddenFiles: next.showHiddenFiles,
    dismissedEverythingBanner: next.dismissedEverythingBanner,
    updateCheckInterval: next.updateCheckInterval,
    logLevel: next.logLevel,
    dateFormat: next.dateFormat,
    showTime: next.showTime,
    showSeconds: next.showSeconds,
    relativeDates: next.relativeDates,
    autoFolderSize: next.autoFolderSize ?? true,
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
