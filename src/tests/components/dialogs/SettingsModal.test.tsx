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
