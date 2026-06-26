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
    })

    const state = useConfigStore.getState()
    expect(state.theme).toBe('light')
    expect(state.showHiddenFiles).toBe(true)
    expect(state.dismissedEverythingBanner).toBe(true)
  })

  it('persists the dismissed banner flag through save_config', () => {
    const saveConfig = vi.fn(() => ({
      theme: 'system' as const,
      showHiddenFiles: false,
      dismissedEverythingBanner: true,
      keybindings: {},
      columns: [],
      layout: {
        detailsVisible: false,
        treeWidth: 'default' as const,
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
            treeWidth: 'default',
            defaultPaneMode: 'dual',
            restoreSession: true,
            zoom: '100',
          },
        }),
      }),
    )
  })
})
