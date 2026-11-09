import { saveConfig } from '@/lib/ipc/commands'
import type { AppConfig, LogLevel } from '@/lib/types/ipc'
import { DEFAULT_DATE_FORMAT, type DateFormat, isDateFormat } from '@/lib/date-format'
import {
  DEFAULT_UPDATE_INTERVAL,
  isUpdateInterval,
  type UpdateInterval,
} from '@/lib/update-intervals'
import { defaultConfigSnapshot, pickConfigSnapshot, useConfigStore } from '@/stores/config-store'
import {
  defaultColumns,
  defaultLayout,
  pickLayoutConfig,
  useLayoutStore,
} from '@/stores/layout-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { migrateKeymap } from '@/lib/keymap'
import { resolvePaneViewMode } from '@/lib/pane-view'

export function defaultAppConfig(): AppConfig {
  return {
    ...defaultConfigSnapshot(),
    keybindings: {},
    columns: defaultColumns,
    layout: defaultLayout,
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
    ...pickConfigSnapshot(config),
    keybindings: keymap.bindings,
    columns: layout.columns,
    layout: { ...pickLayoutConfig(layout), detailsVisible: false },
  }
}

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
    layout: {
      ...defaultLayout,
      ...config.layout,
      detailsVisible: false,
      defaultViewMode: resolvePaneViewMode(
        config.layout?.defaultViewMode,
        defaultLayout.defaultViewMode,
      ),
    },
    columns: config.columns?.length ? config.columns : defaultColumns,
    updateCheckInterval,
    logLevel,
    dateFormat,
    showTime,
    showSeconds,
    relativeDates: config.relativeDates ?? false,
    autoExpandActiveQueueToasts: config.autoExpandActiveQueueToasts ?? false,
  }

  useConfigStore.getState().hydrate(pickConfigSnapshot(next))
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
