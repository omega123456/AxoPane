import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { UpdateBanner } from '@/components/states/UpdateBanner'
import { useUpdaterStore } from '@/stores/updater-store'
import type { AppUpdate } from '@/lib/updater'

function fakeUpdate(overrides: Partial<AppUpdate> = {}): AppUpdate {
  return {
    currentVersion: '0.1.0',
    version: '0.2.0',
    body: 'Fixes',
    date: '2026-06-24',
    downloadAndInstall: vi.fn(() => Promise.resolve()),
    ...overrides,
  }
}

beforeEach(() => {
  useUpdaterStore.getState().dismiss()
})

describe('UpdateBanner', () => {
  it('renders nothing when no update is available', () => {
    const { container } = render(<UpdateBanner />)
    expect(container).toBeEmptyDOMElement()
  })

  it('shows the available version and installs on click', async () => {
    const user = userEvent.setup()
    const downloadAndInstall = vi.fn(() => Promise.resolve())
    vi.spyOn(useUpdaterStore.getState(), 'downloadAndInstall').mockImplementation(downloadAndInstall)
    const update = fakeUpdate()
    useUpdaterStore.getState().setAvailable(update, {
      currentVersion: '0.1.0',
      version: '0.2.0',
      notes: 'Fixes',
    })

    render(<UpdateBanner />)
    expect(screen.getByText(/Update available: 0\.2\.0/)).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: /install & restart/i }))
    expect(downloadAndInstall).toHaveBeenCalledOnce()
  })

  it('renders an install failure message', () => {
    const update = fakeUpdate()
    useUpdaterStore.getState().setAvailable(update, { currentVersion: '0.1.0', version: '0.2.0' })
    useUpdaterStore.setState({ status: 'error', error: 'network down' })

    render(<UpdateBanner />)
    expect(screen.getByText(/Update failed: network down/)).toBeInTheDocument()
    expect(useUpdaterStore.getState().status).toBe('error')
  })

  it('dismisses the banner', async () => {
    const user = userEvent.setup()
    useUpdaterStore.getState().setAvailable(fakeUpdate(), { currentVersion: '0.1.0', version: '0.2.0' })

    render(<UpdateBanner />)
    await user.click(screen.getByRole('button', { name: /dismiss update banner/i }))
    expect(useUpdaterStore.getState().summary).toBeNull()
  })
})
