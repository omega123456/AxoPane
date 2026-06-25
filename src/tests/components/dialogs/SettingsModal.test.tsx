import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ipc } from '@/tests/ipc-mock'
import { useKeymapStore } from '@/stores/keymap-store'
import { useLayoutStore } from '@/stores/layout-store'
import { useSettingsStore } from '@/stores/settings-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

describe('SettingsModal', () => {
  beforeEach(() => {
    ipc.install()
    useSettingsStore.getState().close()
    useKeymapStore.getState().reset()
    useLayoutStore.getState().reset()
    setPlatform('MacIntel')
  })

  it('captures shortcuts, detects conflicts, and applies immediately on macOS', async () => {
    const user = userEvent.setup()
    ipc.override('save_config', (payload) => payload.config)
    useSettingsStore.getState().open('keybindings')

    render(<SettingsModal />)

    const renameButton = screen.getByRole('button', { name: 'Capture Rename shortcut' })
    await user.click(renameButton)
    fireEvent.keyDown(renameButton, { key: 'r', ctrlKey: true })

    await waitFor(() => {
      expect(useKeymapStore.getState().bindings.rename).toEqual(['Ctrl+R'])
    })
    expect(screen.getAllByText('Conflict').length).toBeGreaterThan(0)

    const renameRow = renameButton.closest('tr')
    if (!renameRow) {
      throw new Error('Rename row missing')
    }
    await user.click(within(renameRow).getByRole('button', { name: 'Reset' }))
    await waitFor(() => {
      expect(useKeymapStore.getState().bindings.rename).toEqual(['F2'])
    })
  })

  it('saves column and layout changes on Windows only after Save', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    setPlatform('Win32')
    useSettingsStore.getState().open('columns')

    render(<SettingsModal />)

    await user.click(screen.getByRole('button', { name: /Hidden/i }))
    expect(useLayoutStore.getState().columns.find((column) => column.key === 'created')?.visible).toBe(false)

    await user.click(screen.getByRole('button', { name: 'layout' }))
    await user.click(screen.getByRole('button', { name: 'Details panel' }))
    expect(useLayoutStore.getState().detailsVisible).toBe(true)

    await user.click(screen.getByRole('button', { name: 'Save' }))

    await waitFor(() => {
      expect(useLayoutStore.getState().detailsVisible).toBe(false)
      expect(saveConfig).toHaveBeenCalled()
    })
  })
})

afterAll(() => {
  setPlatform(originalPlatform)
})
