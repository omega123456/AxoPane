import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { UpdatesSettings } from '@/components/dialogs/UpdatesSettings'
import { useUpdaterStore } from '@/stores/updater-store'
import type { AppUpdate } from '@/lib/updater'

function fakeUpdate(): AppUpdate {
  return {
    currentVersion: '0.1.0',
    version: '0.2.0',
    downloadAndInstall: vi.fn(() => Promise.resolve()),
  }
}

beforeEach(() => {
  useUpdaterStore.getState().stopPeriodicCheck()
  useUpdaterStore.getState().dismiss()
})

async function renderUpdatesSettings(
  status?: ReturnType<typeof useUpdaterStore.getState>['status'],
) {
  if (status) {
    useUpdaterStore.setState({ status })
  }

  render(<UpdatesSettings value="1d" onChange={vi.fn()} />)
  await waitFor(() => expect(screen.getByTestId('updates-app-version')).toHaveTextContent('0.1.0'))
}

describe('UpdatesSettings', () => {
  it('shows the running app version', async () => {
    await renderUpdatesSettings()
  })

  it('triggers a manual check from the idle state', async () => {
    const user = userEvent.setup()
    const checkForUpdate = vi.fn(() => Promise.resolve())
    vi.spyOn(useUpdaterStore.getState(), 'checkForUpdate').mockImplementation(checkForUpdate)

    render(<UpdatesSettings value="1d" onChange={vi.fn()} />)
    await user.click(screen.getByTestId('updates-check-button'))
    expect(checkForUpdate).toHaveBeenCalledWith(true)
  })

  it('installs an available update', async () => {
    const user = userEvent.setup()
    const downloadAndInstall = vi.fn(() => Promise.resolve())
    vi.spyOn(useUpdaterStore.getState(), 'downloadAndInstall').mockImplementation(
      downloadAndInstall,
    )
    useUpdaterStore.getState().setAvailable(fakeUpdate(), {
      currentVersion: '0.1.0',
      version: '0.2.0',
    })

    render(<UpdatesSettings value="1d" onChange={vi.fn()} />)
    expect(screen.getByTestId('updates-available')).toHaveTextContent('Version 0.2.0 is available')
    await user.click(screen.getByTestId('updates-install-button'))
    expect(downloadAndInstall).toHaveBeenCalledOnce()
  })

  it.each([
    ['checking', 'updates-checking'],
    ['up-to-date', 'updates-up-to-date'],
    ['installing', 'updates-installing'],
  ] as const)('renders the %s status', async (status, testid) => {
    await renderUpdatesSettings(status)
    expect(screen.getByTestId(testid)).toBeInTheDocument()
  })

  it('renders an error status with a retry affordance', async () => {
    useUpdaterStore.setState({ status: 'error', error: 'network down' })
    await renderUpdatesSettings()
    expect(screen.getByTestId('updates-error')).toHaveTextContent('network down')
    expect(screen.getByTestId('updates-check-button')).toBeInTheDocument()
  })

  it('changes the check frequency', async () => {
    const user = userEvent.setup()
    const onChange = vi.fn()
    render(<UpdatesSettings value="1d" onChange={onChange} />)
    await user.selectOptions(screen.getByLabelText('Update check frequency'), 'off')
    expect(onChange).toHaveBeenCalledWith('off')
  })
})
