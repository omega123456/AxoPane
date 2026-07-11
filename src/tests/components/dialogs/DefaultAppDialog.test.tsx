import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { DefaultAppDialog } from '@/components/dialogs/DefaultAppDialog'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import { ipc } from '@/tests/ipc-mock'
import type { MacApp } from '@/lib/types/ipc'

const fixtureApps: MacApp[] = [
  {
    name: 'Fixture Preview',
    bundlePath: '/Applications/Fixture Preview.app',
    bundleId: 'com.example.fixture-preview',
    iconDataUrl: 'data:image/png;base64,RkFLRQ==',
  },
  {
    name: 'Fixture Text Edit',
    bundlePath: '/Applications/Fixture Text Edit.app',
    bundleId: 'com.example.fixture-textedit',
    iconDataUrl: null,
  },
]

beforeEach(() => {
  useDefaultAppDialogStore.getState().close()
})

afterEach(() => {
  ipc.reset()
})

function openDialog(apps: MacApp[] = fixtureApps) {
  act(() => {
    useDefaultAppDialogStore.getState().open({
      filePath: '/Users/example/Report.pdf',
      fileName: 'Report.pdf',
      apps,
    })
  })
}

describe('DefaultAppDialog', () => {
  it('renders nothing when closed', () => {
    render(<DefaultAppDialog />)
    expect(screen.queryByRole('dialog')).not.toBeInTheDocument()
  })

  it('loads and lists applications, falling back the icon on load error', async () => {
    render(<DefaultAppDialog />)
    openDialog()

    expect(await screen.findByText('Fixture Preview')).toBeInTheDocument()
    expect(screen.getByText('Fixture Text Edit')).toBeInTheDocument()

    const img = screen.getByRole('option', { name: 'Fixture Preview' }).querySelector('img')
    expect(img).toBeInTheDocument()
  })

  it('shows an empty state when no applications are found', async () => {
    render(<DefaultAppDialog />)
    openDialog([])

    expect(await screen.findByText('No applications were found.')).toBeInTheDocument()
  })

  it('selects an app, confirms via Change All, and closes the dialog', async () => {
    const user = userEvent.setup()
    let receivedPayload: unknown
    ipc.override('set_default_application', (payload) => {
      receivedPayload = payload
      return { handled: true, message: 'default-application-set' }
    })

    render(<DefaultAppDialog />)
    openDialog()

    const option = await screen.findByRole('option', { name: 'Fixture Preview' })
    await user.click(option)

    const confirmButton = screen.getByRole('button', { name: 'Change All…' })
    expect(confirmButton).toBeEnabled()
    await user.click(confirmButton)

    await waitFor(() => {
      expect(useDefaultAppDialogStore.getState().dialog).toBeNull()
    })
    expect(receivedPayload).toEqual({
      path: '/Users/example/Report.pdf',
      bundlePath: '/Applications/Fixture Preview.app',
    })
  })

  it('only selects an application while browsing, even on a double click', async () => {
    const user = userEvent.setup()
    const setDefaultApplication = vi.fn(() => ({
      handled: true,
      message: 'default-application-set',
    }))
    ipc.override('set_default_application', setDefaultApplication)

    render(<DefaultAppDialog />)
    openDialog()

    await user.dblClick(await screen.findByRole('option', { name: 'Fixture Preview' }))

    expect(screen.getByRole('option', { name: 'Fixture Preview' })).toHaveAttribute(
      'aria-selected',
      'true',
    )
    expect(setDefaultApplication).not.toHaveBeenCalled()
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()
  })

  it('shows the mapped inline error and stays open when the backend reports a failure', async () => {
    const user = userEvent.setup()
    ipc.override('set_default_application', {
      handled: false,
      message: 'default-application-rejected-dynamic-type',
    })

    render(<DefaultAppDialog />)
    openDialog()

    const option = await screen.findByRole('option', { name: 'Fixture Preview' })
    await user.click(option)
    await user.click(screen.getByRole('button', { name: 'Change All…' }))

    const alert = await screen.findByRole('alert')
    expect(alert).toHaveTextContent('Fixture Preview')
    expect(alert).toHaveTextContent('.pdf')
    expect(alert).toHaveAttribute('aria-live', 'assertive')

    // Dialog stays open and the confirm button is re-enabled for a retry.
    expect(screen.getByRole('dialog', { name: 'Set Default Application' })).toBeInTheDocument()
    expect(useDefaultAppDialogStore.getState().dialog).not.toBeNull()
    expect(screen.getByRole('button', { name: 'Change All…' })).toBeEnabled()
  })

  it('clears the inline error when a different app is selected, and can then succeed', async () => {
    const user = userEvent.setup()
    ipc.override('set_default_application', {
      handled: false,
      message: 'default-application-write-failed',
    })

    render(<DefaultAppDialog />)
    openDialog()

    const firstOption = await screen.findByRole('option', { name: 'Fixture Preview' })
    await user.click(firstOption)
    await user.click(screen.getByRole('button', { name: 'Change All…' }))
    await screen.findByRole('alert')

    ipc.override('set_default_application', { handled: true, message: 'default-application-set' })

    const secondOption = screen.getByRole('option', { name: 'Fixture Text Edit' })
    await user.click(secondOption)
    expect(screen.queryByRole('alert')).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Change All…' }))

    await waitFor(() => {
      expect(useDefaultAppDialogStore.getState().dialog).toBeNull()
    })
  })

  it('disables Change All until an app is selected, and closes on Cancel', async () => {
    const user = userEvent.setup()
    render(<DefaultAppDialog />)
    openDialog()

    await screen.findByText('Fixture Preview')
    expect(screen.getByRole('button', { name: 'Change All…' })).toBeDisabled()

    await user.click(screen.getByRole('button', { name: 'Cancel' }))
    expect(useDefaultAppDialogStore.getState().dialog).toBeNull()
  })

  it('closes on Escape', async () => {
    const user = userEvent.setup()
    render(<DefaultAppDialog />)
    openDialog()

    await screen.findByText('Fixture Preview')
    await user.keyboard('{Escape}')
    expect(useDefaultAppDialogStore.getState().dialog).toBeNull()
  })
})
