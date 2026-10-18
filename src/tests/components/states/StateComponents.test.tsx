import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { vi } from 'vitest'
import { EmptyState } from '@/components/states/EmptyState'
import { ErrorState } from '@/components/states/ErrorState'
import { LoadingSkeleton } from '@/components/states/LoadingSkeleton'
import { PermissionDenied } from '@/components/states/PermissionDenied'
import { ipc } from '../../ipc-mock'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('state components', () => {
  it('renders static loading rows with a status role', () => {
    render(<LoadingSkeleton />)

    const status = screen.getByRole('status', { name: 'Loading folder' })
    expect(status).toBeInTheDocument()
    expect(status.querySelector('.animate-pulse')).toBeNull()
  })

  it('offers a new folder from the empty state', async () => {
    const user = userEvent.setup()
    const onNewFolder = vi.fn()
    render(<EmptyState onNewFolder={onNewFolder} />)

    expect(screen.getByRole('status', { name: 'Empty folder' })).toBeInTheDocument()
    expect(screen.getByText('This folder is empty')).toBeInTheDocument()
    expect(screen.getByText('Drag files here, or create a folder.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'New folder' }))

    expect(onNewFolder).toHaveBeenCalledOnce()
  })

  it('renders the empty state without an action where folders cannot be created', () => {
    render(<EmptyState />)

    expect(screen.getByText('Nothing is here.')).toBeInTheDocument()
    expect(screen.queryByRole('button')).toBeNull()
  })

  it('names the folder in the error state and retries', async () => {
    const user = userEvent.setup()
    const onRetry = vi.fn()
    render(<ErrorState path="D:\Archive" message="The drive was disconnected." onRetry={onRetry} />)

    expect(screen.getByRole('alert')).toHaveTextContent("Couldn't open D:\\Archive")
    expect(screen.getByText('The drive was disconnected.')).toBeInTheDocument()
    expect(screen.queryByRole('button', { name: 'Go up' })).toBeNull()
    await user.click(screen.getByRole('button', { name: 'Try again' }))

    expect(onRetry).toHaveBeenCalledOnce()
  })

  it('offers to restart as administrator on Windows', async () => {
    setPlatform('Win32')
    const user = userEvent.setup()
    const restart = vi.fn()
    ipc.override('restart_as_admin', restart)
    render(<PermissionDenied />)

    expect(screen.getByRole('alert', { name: 'Permission denied' })).toHaveTextContent(
      "You don't have access",
    )
    expect(screen.getByText('Windows blocks this folder for your account.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Open as administrator' }))

    await waitFor(() => expect(restart).toHaveBeenCalledOnce())
  })

  it('opens the Full Disk Access settings on macOS', async () => {
    setPlatform('MacIntel')
    const user = userEvent.setup()
    const openSettings = vi.fn()
    ipc.override('open_full_disk_access_settings', openSettings)
    render(<PermissionDenied />)

    expect(screen.getByText('macOS needs Full Disk Access for AxoPane.')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Open System Settings' }))

    await waitFor(() => expect(openSettings).toHaveBeenCalledOnce())
  })

  it('logs a permission fix that fails', async () => {
    setPlatform('MacIntel')
    const user = userEvent.setup()
    const logged = vi.fn()
    ipc.override('log_frontend', logged)
    ipc.override('open_full_disk_access_settings', () => {
      throw new Error('denied')
    })
    render(<PermissionDenied />)

    await user.click(screen.getByRole('button', { name: 'Open System Settings' }))

    await waitFor(() =>
      expect(logged).toHaveBeenCalledWith(
        expect.objectContaining({ message: 'permission fix failed' }),
      ),
    )
  })
})
