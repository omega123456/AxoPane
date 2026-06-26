import { saveConfig } from '@/lib/ipc/commands'
import type { AppConfig, ThemePreference } from '@/lib/types/ipc'
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
  }
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
      treeWidth: layout.treeWidth,
      defaultPaneMode: layout.defaultPaneMode,
      restoreSession: layout.restoreSession,
      zoom: layout.zoom,
    },
  }
}

export function hydrateAppConfig(config: AppConfig) {
  const { bindings, migrated } = migrateKeymap(config.keybindings)
  const next = {
    ...defaultAppConfig(),
    ...config,
    keybindings: bindings,
    layout: { ...defaultLayout, ...config.layout, detailsVisible: false },
    columns: config.columns?.length ? config.columns : defaultColumns,
  }

  useConfigStore.getState().hydrate({
    theme: next.theme as ThemePreference,
    showHiddenFiles: next.showHiddenFiles,
    dismissedEverythingBanner: next.dismissedEverythingBanner,
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
