import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { EverythingBanner } from '@/components/states/EverythingBanner'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'

const originalUserAgent = navigator.userAgent

function setUserAgent(value: string) {
  Object.defineProperty(navigator, 'userAgent', { value, configurable: true })
}

beforeEach(() => {
  ipc.install()
  useConfigStore.getState().reset()
  usePanesStore.getState().reset()
  setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64)')
})

afterEach(() => {
  setUserAgent(originalUserAgent)
})

describe('EverythingBanner', () => {
  it('shows on Windows when Everything is unavailable and persists dismissal', async () => {
    const user = userEvent.setup()
    usePanesStore.setState({ everythingStatus: { status: 'unavailable', isAvailable: false } })

    const saveConfig = vi.fn(() => ({
      theme: 'system' as const,
      showHiddenFiles: false,
      dismissedEverythingBanner: true,
      updateCheckInterval: '1d' as const,
      logLevel: 'info' as const,
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

    render(<EverythingBanner />)
    expect(screen.getByRole('status', { name: 'Everything unavailable' })).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Dismiss Everything banner' }))

    expect(useConfigStore.getState().dismissedEverythingBanner).toBe(true)
    expect(saveConfig).toHaveBeenCalledOnce()
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })

  it('does not show when Everything is available', () => {
    usePanesStore.setState({ everythingStatus: { status: 'available', isAvailable: true } })
    render(<EverythingBanner />)
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })

  it('does not show on non-Windows platforms', () => {
    setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)')
    usePanesStore.setState({ everythingStatus: { status: 'unavailable', isAvailable: false } })
    render(<EverythingBanner />)
    expect(screen.queryByRole('status', { name: 'Everything unavailable' })).not.toBeInTheDocument()
  })
})
