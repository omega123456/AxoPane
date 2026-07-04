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
  it('fires up/refresh, and toggles theme', async () => {
    const user = userEvent.setup()
    const goUp = vi.fn(() => Promise.resolve())
    const reloadPane = vi.fn(() => Promise.resolve())
    const setTheme = vi.fn()
    usePanesStore.setState((state) => ({
      goUp,
      reloadPane,
      panes: {
        ...state.panes,
        left: { ...state.panes.left, path: 'C:\\Users', filterApplied: '' },
      },
    }))

    render(<CommandBar theme="dark" setTheme={setTheme} />)

    await user.click(screen.getByRole('button', { name: 'Up' }))
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    expect(goUp).toHaveBeenCalledWith('left')
    expect(reloadPane).toHaveBeenCalledWith('left')

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
