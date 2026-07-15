import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { CommandBar } from '@/components/shell/CommandBar'
import { usePanesStore } from '@/stores/panes-store'

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
})

describe('CommandBar', () => {
  it('keeps navigation controls out of the global bar and toggles theme', async () => {
    const user = userEvent.setup()
    const setTheme = vi.fn()

    render(<CommandBar theme="dark" setTheme={setTheme} />)

    expect(screen.queryByRole('button', { name: /^(back|up|refresh)$/i })).not.toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Light theme' }))
    expect(setTheme).toHaveBeenCalledWith('light')
  })

  it('reflects the hidden-files state and theme label', () => {
    usePanesStore.setState((state) => ({
      showHiddenFiles: true,
      panes: { ...state.panes, left: { ...state.panes.left, filterApplied: 'mkv' } },
    }))
    render(<CommandBar theme="light" setTheme={vi.fn()} />)
    expect(screen.getByRole('button', { name: /hidden files/i })).toHaveAttribute(
      'aria-pressed',
      'true',
    )
    expect(screen.getByRole('button', { name: 'Dark theme' })).toBeInTheDocument()
  })

  it('toggles the global hidden-files setting through the store action', async () => {
    const user = userEvent.setup()
    const setShowHiddenFiles = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ showHiddenFiles: false, setShowHiddenFiles })

    render(<CommandBar theme="light" setTheme={vi.fn()} />)
    const toggle = screen.getByRole('button', { name: /hidden files/i })
    expect(toggle).toHaveAttribute('aria-pressed', 'false')

    await user.click(toggle)
    expect(setShowHiddenFiles).toHaveBeenCalledWith(true)
  })
})
