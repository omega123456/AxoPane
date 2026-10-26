import { act, render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { EverythingBanner } from '@/components/states/EverythingBanner'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

beforeEach(() => {
  ipc.install()
  useConfigStore.getState().reset()
  usePanesStore.getState().reset()
  setPlatform('Win32')
})

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('EverythingBanner', () => {
  it('shows on Windows when Everything is unavailable and persists dismissal', async () => {
    const user = userEvent.setup()
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'everything', isAvailable: false, state: 'unavailable' },
    })

    const saveConfig = vi.fn(() => ({
      theme: 'system' as const,
      showHiddenFiles: false,
      dismissedEverythingBanner: true,
      updateCheckInterval: '1d' as const,
      logLevel: 'info' as const,
      dateFormat: 'ymd' as const,
      showTime: false,
      showSeconds: false,
      relativeDates: false,
      autoFolderSize: true,
      autoExpandActiveQueueToasts: false,
      favourites: [],
      favouriteNames: {},
      favouritesCollapsed: false,
      keybindings: {},
      columns: [],
      layout: {
        detailsVisible: false,
        treeWidthPx: 204,
        favouritesHeightPx: 112,
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
        defaultViewMode: 'details' as const,
        restoreSession: true,
        zoom: '100' as const,
      },
    }))
    ipc.override('save_config', saveConfig)

    render(<EverythingBanner />)
    expect(screen.getByRole('status', { name: 'Everything unavailable' })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Dismiss Everything banner' }))

    expect(useConfigStore.getState().dismissedEverythingBanner).toBe(true)
    expect(saveConfig).toHaveBeenCalledOnce()
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })

  it('does not show when Everything is available', () => {
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'everything', isAvailable: true, state: 'upToDate' },
    })
    render(<EverythingBanner />)
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })

  it('does not show for other backends or before the status loads', () => {
    render(<EverythingBanner />)
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
    act(() =>
      usePanesStore.setState({
        folderSizeIndexStatus: { backend: 'macIndex', isAvailable: false, state: 'unavailable' },
      }),
    )
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })

  it('does not show on non-Windows platforms', () => {
    setPlatform('MacIntel')
    usePanesStore.setState({
      folderSizeIndexStatus: { backend: 'everything', isAvailable: false, state: 'unavailable' },
    })
    render(<EverythingBanner />)
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })
})
