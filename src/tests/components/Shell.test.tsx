import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { CommandBar } from '@/components/shell/CommandBar'
import { StatusBar } from '@/components/shell/StatusBar'
import { usePanesStore } from '@/stores/panes-store'

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
})

describe('CommandBar', () => {
  it('shows the path, fires up/refresh, and toggles theme', async () => {
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
    expect(screen.getByText('C:\\Users')).toBeInTheDocument()

    await user.click(screen.getByRole('button', { name: 'Up' }))
    await user.click(screen.getByRole('button', { name: 'Refresh' }))
    expect(goUp).toHaveBeenCalledWith('left')
    expect(reloadPane).toHaveBeenCalledWith('left')

    await user.click(screen.getByRole('button', { name: 'Light theme' }))
    expect(setTheme).toHaveBeenCalledWith('light')
  })

  it('shows the active filter when one is applied and hidden-files state', () => {
    usePanesStore.setState((state) => ({
      showHiddenFiles: true,
      panes: { ...state.panes, left: { ...state.panes.left, filterApplied: 'mkv' } },
    }))
    render(<CommandBar theme="light" setTheme={vi.fn()} />)
    expect(screen.getByText('Filter: mkv')).toBeInTheDocument()
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

describe('StatusBar', () => {
  const basePane = () => usePanesStore.getState().panes.left

  it('renders counts, focused entry, and volume free space', () => {
    render(
      <StatusBar
        activePane={{ ...basePane(), path: 'C:\\Users', typing: false }}
        summary={{
          itemCount: 3,
          selectionCount: 1,
          focusedEntry: {
            id: 'r',
            name: 'Report.txt',
            path: 'C:\\Users\\Report.txt',
            isDir: false,
            sizeBytes: 1024,
            itemCount: null,
            typeLabel: 'TXT file',
            modifiedAt: null,
            createdAt: null,
            attributes: [],
            isHidden: false,
            isSystem: false,
          },
          volume: {
            mountRoot: 'C:\\',
            label: 'System',
            totalBytes: 4_000_000_000_000,
            freeBytes: 412_000_000_000,
            isNetwork: false,
          },
        }}
      />,
    )

    expect(screen.getByText('3 items')).toBeInTheDocument()
    expect(screen.getByText('1 selected')).toBeInTheDocument()
    expect(screen.getByText(/Report.txt/)).toBeInTheDocument()
    expect(screen.getByText(/free of/)).toBeInTheDocument()
  })

  it('shows a filtering indicator and a folder focus label without a volume', () => {
    render(
      <StatusBar
        activePane={{ ...basePane(), path: 'C:\\Users', typing: true }}
        summary={{
          itemCount: 0,
          selectionCount: 0,
          focusedEntry: {
            id: 'd',
            name: 'Docs',
            path: 'C:\\Users\\Docs',
            isDir: true,
            sizeBytes: null,
            itemCount: 2,
            typeLabel: 'Folder',
            modifiedAt: null,
            createdAt: null,
            attributes: [],
            isHidden: false,
            isSystem: false,
          },
        }}
      />,
    )

    expect(screen.getByText('Filtering…')).toBeInTheDocument()
    expect(screen.getByText(/· folder/)).toBeInTheDocument()
  })
})
