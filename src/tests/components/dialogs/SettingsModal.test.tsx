import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import { ipc } from '@/tests/ipc-mock'
import { useConfigStore } from '@/stores/config-store'
import { useKeymapStore } from '@/stores/keymap-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { useThemeStore } from '@/stores/theme-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

describe('SettingsModal', () => {
  beforeEach(() => {
    ipc.install()
    ipc.override('save_config', (payload) => payload.config)
    useSettingsStore.getState().close()
    useConfigStore.getState().reset()
    useKeymapStore.getState().reset()
    useLayoutStore.getState().reset()
    usePanesStore.getState().reset()
    useThemeStore.getState().setThemePreference('system')
    setPlatform('MacIntel')
  })

  it('captures shortcuts, detects conflicts, and applies immediately on macOS', async () => {
    const user = userEvent.setup()
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

  it('renders reserved clipboard commands as non-remappable system defaults', async () => {
    useSettingsStore.getState().open('keybindings')

    render(<SettingsModal />)

    // The footer resolves the app version asynchronously; wait for it to settle
    // so the state update lands inside act().
    await screen.findByText('build 0.1.0', { exact: false })

    // Reserved commands have no capture or reset affordance and are labelled as
    // platform defaults rather than ever flagged as conflicts.
    expect(screen.queryByRole('button', { name: 'Capture Copy shortcut' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Capture Cut shortcut' })).toBeNull()
    expect(screen.queryByRole('button', { name: 'Capture Paste shortcut' })).toBeNull()

    const copyRow = screen.getByText('Copy').closest('tr')
    if (!copyRow) {
      throw new Error('Copy row missing')
    }
    expect(within(copyRow).getByText('System default')).toBeInTheDocument()
    expect(within(copyRow).queryByRole('button', { name: 'Reset' })).toBeNull()
    expect(within(copyRow).queryByText('Conflict')).toBeNull()
  })

  it('keeps theme, hidden-file, and column changes in draft on Windows until Save', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    setPlatform('Win32')
    useSettingsStore.getState().open('layout')

    render(<SettingsModal />)

    await user.click(screen.getByRole('switch', { name: 'Show hidden files' }))
    await user.click(screen.getByRole('radio', { name: 'Dark' }))
    await user.click(screen.getByRole('button', { name: 'Columns' }))
    await user.click(screen.getByRole('switch', { name: 'Created column' }))

    expect(usePanesStore.getState().showHiddenFiles).toBe(false)
    expect(useThemeStore.getState().preference).toBe('system')
    expect(useLayoutStore.getState().detailsVisible).toBe(false)
    expect(
      useLayoutStore.getState().columns.find((column) => column.key === 'created')?.visible,
    ).toBe(false)

    await user.click(screen.getByRole('button', { name: 'Save changes' }))

    await waitFor(() => {
      expect(usePanesStore.getState().showHiddenFiles).toBe(true)
      expect(useThemeStore.getState().preference).toBe('dark')
      expect(useLayoutStore.getState().detailsVisible).toBe(false)
      expect(
        useLayoutStore.getState().columns.find((column) => column.key === 'created')?.visible,
      ).toBe(true)
      expect(saveConfig).toHaveBeenCalled()
    })
  })

  it('persists the update check frequency from the Updates section', async () => {
    const user = userEvent.setup()
    const saveConfig = vi.fn((payload) => payload.config)
    ipc.override('save_config', saveConfig)
    useSettingsStore.getState().open('updates')

    render(<SettingsModal />)

    await user.selectOptions(screen.getByLabelText('Update check frequency'), 'Every hour')

    await waitFor(() => {
      expect(useConfigStore.getState().updateCheckInterval).toBe('1h')
      expect(saveConfig).toHaveBeenCalledWith(
        expect.objectContaining({
          config: expect.objectContaining({ updateCheckInterval: '1h' }),
        }),
      )
    })
  })

  it('applies a zoom selection immediately on macOS', async () => {
    const user = userEvent.setup()
    useSettingsStore.getState().open('layout')

    const setProperty = vi.spyOn(document.documentElement.style, 'setProperty')
    render(<SettingsModal />)

    await user.selectOptions(screen.getByRole('combobox', { name: 'Zoom' }), '125')

    await waitFor(() => {
      expect(useLayoutStore.getState().zoom).toBe('125')
    })
    expect(setProperty).toHaveBeenCalledWith('zoom', '1.25')
    setProperty.mockRestore()
  })

  it('resets the draft back to defaults before saving on Windows', async () => {
    const user = userEvent.setup()
    setPlatform('Win32')
    useSettingsStore.getState().open('layout')

    render(<SettingsModal />)

    await user.click(screen.getByRole('switch', { name: 'Show hidden files' }))
    await user.click(screen.getByRole('radio', { name: 'Dark' }))
    await user.click(screen.getByRole('button', { name: 'Reset to defaults' }))

    expect(screen.getByRole('switch', { name: 'Show hidden files' })).toHaveAttribute(
      'aria-checked',
      'false',
    )
    expect(screen.getByRole('radio', { name: 'System' })).toHaveAttribute('aria-checked', 'true')
    expect(usePanesStore.getState().showHiddenFiles).toBe(false)
    expect(useThemeStore.getState().preference).toBe('system')
    expect(useLayoutStore.getState().detailsVisible).toBe(false)
  })
})

afterAll(() => {
  setPlatform(originalPlatform)
})
