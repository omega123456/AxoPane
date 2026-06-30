import { render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach } from 'vitest'
import { DefaultAppDialog } from '@/components/dialogs/DefaultAppDialog'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import { ipc } from '@/tests/ipc-mock'

beforeEach(() => {
  useDefaultAppDialogStore.getState().close()
})

afterEach(() => {
  ipc.reset()
})

function openDialog() {
  useDefaultAppDialogStore.getState().open({
    filePath: '/Users/example/Report.pdf',
    fileName: 'Report.pdf',
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
    ipc.override('list_applications', { apps: [] })
    render(<DefaultAppDialog />)
    openDialog()

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
