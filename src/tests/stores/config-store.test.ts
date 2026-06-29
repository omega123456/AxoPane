import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { useConfigStore } from '@/stores/config-store'

beforeEach(() => {
  ipc.install()
  useConfigStore.getState().reset()
})

describe('config-store', () => {
  it('hydrates from an AppConfig', () => {
    useConfigStore.getState().hydrate({
      theme: 'light',
      showHiddenFiles: true,
      dismissedEverythingBanner: true,
      updateCheckInterval: '1d',
    })

    const state = useConfigStore.getState()
    expect(state.theme).toBe('light')
    expect(state.showHiddenFiles).toBe(true)
    expect(state.dismissedEverythingBanner).toBe(true)
  })

  it('persists the update check interval through save_config', async () => {
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)

    await useConfigStore.getState().setUpdateCheckInterval('5h')

    expect(useConfigStore.getState().updateCheckInterval).toBe('5h')
    expect(saveConfig).toHaveBeenCalledWith(
      expect.objectContaining({
        config: expect.objectContaining({ updateCheckInterval: '5h' }),
      }),
    )
  })

  it('persists the dismissed banner flag through save_config', () => {
    const saveConfig = vi.fn(() => ({
      theme: 'system' as const,
      showHiddenFiles: false,
      dismissedEverythingBanner: true,
      updateCheckInterval: '1d' as const,
      keybindings: {},
      columns: [],
      layout: {
        detailsVisible: false,
        treeWidthPx: 204,
        paneSplit: 0.5,
        columnWidths: {
          name: 320,
          size: 96,
          items: 72,
          type: 136,
          modified: 128,
          created: 128,
        },
        defaultPaneMode: 'dual' as const,
        restoreSession: true,
        zoom: '100' as const,
      },
    }))
    ipc.override('save_config', saveConfig)

    useConfigStore.getState().dismissEverythingBanner()

    expect(useConfigStore.getState().dismissedEverythingBanner).toBe(true)
    expect(saveConfig).toHaveBeenCalledWith(
      expect.objectContaining({
        config: expect.objectContaining({
          theme: 'system',
          showHiddenFiles: false,
          dismissedEverythingBanner: true,
          layout: {
            detailsVisible: false,
            treeWidthPx: 204,
            paneSplit: 0.5,
            columnWidths: {
              name: 320,
              size: 96,
              items: 72,
              type: 136,
              modified: 128,
              created: 128,
            },
            defaultPaneMode: 'dual',
            restoreSession: true,
            zoom: '100',
          },
        }),
      }),
    )
  })
})
