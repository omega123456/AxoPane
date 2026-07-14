import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { BreadcrumbBar, splitPath } from '@/components/pane/BreadcrumbBar'
import { useActionDialogStore } from '@/stores/action-dialog-store'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneState } from '@/types/pane'
import type { DirectoryEntry } from '@/lib/types/ipc'

function folderEntries(count: number): DirectoryEntry[] {
  return Array.from({ length: count }, (_, index) => ({
    id: `dir-${index}`,
    name: `dir-${index}`,
    path: `C:\\Users\\Omega\\dir-${index}`,
    isDir: true,
    sizeBytes: null,
    itemCount: 1,
    typeLabel: 'Folder',
    modifiedAt: null,
    createdAt: null,
    attributes: [],
    isHidden: false,
    isSystem: false,
  }))
}

function pane(path: string, entries: DirectoryEntry[] = []): PaneState {
  return {
    id: 'left',
    title: 'Left pane',
    path,
    entries,
    focusedEntryId: null,
    sortKey: 'name',
    sortDirection: 'asc',
    filterDraft: '',
    filterApplied: '',
    typing: false,
    loading: false,
    itemsSortStatus: 'idle',
    error: null,
    listRequestId: 0,
    scrollPositions: {},
  }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useConfigStore.getState().reset()
  useActionDialogStore.getState().close()
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('splitPath', () => {
  it('splits Windows paths into cumulative segments with the drive root preserved', () => {
    expect(splitPath('C:\\Users\\Omega\\Documents')).toEqual([
      { label: 'C:', path: 'C:\\' },
      { label: 'Users', path: 'C:\\Users' },
      { label: 'Omega', path: 'C:\\Users\\Omega' },
      { label: 'Documents', path: 'C:\\Users\\Omega\\Documents' },
    ])
  })

  it('handles a bare Windows drive root', () => {
    expect(splitPath('C:\\')).toEqual([{ label: 'C:', path: 'C:\\' }])
    expect(splitPath('C:')).toEqual([{ label: 'C:', path: 'C:\\' }])
  })

  it('splits UNC paths from the share root without synthetic prefix segments', () => {
    expect(splitPath('\\\\raspberry.pi\\share\\TV')).toEqual([
      { label: '\\\\raspberry.pi\\share', path: '\\\\raspberry.pi\\share' },
      { label: 'TV', path: '\\\\raspberry.pi\\share\\TV' },
    ])
  })

  it('normalizes extended UNC paths before splitting breadcrumbs', () => {
    expect(splitPath('\\\\?\\UNC\\raspberry.pi\\share')).toEqual([
      { label: '\\\\raspberry.pi\\share', path: '\\\\raspberry.pi\\share' },
    ])
  })

  it('splits POSIX paths into cumulative segments under the root', () => {
    expect(splitPath('/home/omega/dev')).toEqual([
      { label: '/', path: '/' },
      { label: 'home', path: '/home' },
      { label: 'omega', path: '/home/omega' },
      { label: 'dev', path: '/home/omega/dev' },
    ])
  })

  it('falls back to a single dot segment for an empty path', () => {
    expect(splitPath('')).toEqual([{ label: '.', path: '.' }])
  })
})

describe('BreadcrumbBar navigation', () => {
  it('keeps full breadcrumb labels clickable when width measurement is unavailable', () => {
    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)

    expect(screen.getByRole('button', { name: 'C:' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Users' })).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Omega' })).toBeInTheDocument()
    expect(screen.queryByText('..')).not.toBeInTheDocument()
  })

  it('navigates the pane to the clicked segment path', async () => {
    const user = userEvent.setup()
    const navigatePane = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ navigatePane })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)
    await user.click(screen.getByRole('button', { name: 'Users' }))

    expect(navigatePane).toHaveBeenCalledWith('left', 'C:\\Users')
  })

  it('navigates UNC breadcrumbs using real network paths', async () => {
    const user = userEvent.setup()
    const navigatePane = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ navigatePane })

    render(<BreadcrumbBar pane={pane('\\\\?\\UNC\\raspberry.pi\\share\\TV')} isActive />)
    await user.click(screen.getByRole('button', { name: '\\\\raspberry.pi\\share' }))
    await user.click(screen.getByRole('button', { name: 'TV' }))

    expect(navigatePane).toHaveBeenNthCalledWith(1, 'left', '\\\\raspberry.pi\\share')
    expect(navigatePane).toHaveBeenNthCalledWith(2, 'left', '\\\\raspberry.pi\\share\\TV')
  })

  it('turns the breadcrumbs into a full-path editor from the trailing empty area', async () => {
    const user = userEvent.setup()
    const navigatePane = vi.fn(() => Promise.resolve())
    usePanesStore.setState({ navigatePane })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)
    await user.dblClick(screen.getByRole('navigation', { name: 'Left pane path' }))

    const input = screen.getByRole('textbox', { name: 'Left pane path' })
    expect(input).toHaveValue('C:\\Users\\Omega')

    await user.clear(input)
    await user.type(input, 'D:\\Media{Enter}')

    expect(navigatePane).toHaveBeenCalledWith('left', 'D:\\Media')
  })

  it('opens the full-path editor from a right-click on any breadcrumb', async () => {
    const user = userEvent.setup()

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)
    await user.pointer({
      keys: '[MouseRight]',
      target: screen.getByRole('button', { name: 'Users' }),
    })

    expect(screen.getByRole('textbox', { name: 'Left pane path' })).toHaveValue('C:\\Users\\Omega')
  })

  it('shows a hover tooltip on the refresh button', () => {
    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)

    expect(screen.getByRole('button', { name: 'Refresh Left pane' })).toHaveAttribute(
      'title',
      'Refresh',
    )
  })

  it('renders a visual collapse marker and title tooltip for truncated crumbs when measured widths are narrow', () => {
    const originalGetComputedStyle = window.getComputedStyle

    vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect').mockImplementation(function mockRect(
      this: HTMLElement,
    ) {
      if (this.getAttribute('aria-label') === 'Left pane path') {
        return DOMRect.fromRect({ width: 195 })
      }

      return DOMRect.fromRect()
    })
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
      font: '',
      measureText: (label: string) => ({ width: label.length * 10 }) as TextMetrics,
    } as CanvasRenderingContext2D)
    vi.spyOn(window, 'getComputedStyle').mockImplementation((element: Element) => {
      const style = originalGetComputedStyle(element)
      if (element.getAttribute('aria-label') === 'Left pane path') {
        Object.defineProperty(style, 'font', {
          configurable: true,
          value: '12px ui-monospace',
        })
      }
      return style
    })

    render(<BreadcrumbBar pane={pane('C:\\Alpha\\Bravo\\Charlie')} isActive />)

    expect(screen.getByTestId('breadcrumb-collapse-marker')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: 'Br' })).toHaveAttribute('title', 'Bravo')
    expect(screen.getByRole('button', { name: 'Charlie' })).toBeInTheDocument()
  })
})

describe('BreadcrumbBar calculate-all-sizes button', () => {
  const buttonName = 'Calculate all folder sizes in Left pane'

  it('shows the button when Everything is unavailable and opens the confirmation dialog', async () => {
    const user = userEvent.setup()
    usePanesStore.setState({
      everythingStatus: { status: 'unavailable', isAvailable: false },
    })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)
    const button = screen.getByRole('button', { name: buttonName })
    expect(button).toHaveAttribute('title', 'Calculate all folder sizes')
    await user.click(button)

    expect(useActionDialogStore.getState().dialog).toEqual({
      kind: 'calculateAllSizes',
      paneId: 'left',
    })
  })

  it('shows the button when Everything is available but auto folder size is off', () => {
    usePanesStore.setState({
      everythingStatus: { status: 'available', isAvailable: true },
    })
    useConfigStore.setState({ autoFolderSize: false })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)

    expect(screen.getByRole('button', { name: buttonName })).toBeInTheDocument()
  })

  it('hides the button when Everything is available and auto folder size is on', () => {
    usePanesStore.setState({
      everythingStatus: { status: 'available', isAvailable: true },
    })
    useConfigStore.setState({ autoFolderSize: true })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega')} isActive />)

    expect(screen.queryByRole('button', { name: buttonName })).not.toBeInTheDocument()
  })

  it('shows the button when the pane holds more than 500 folders, even with auto sizing on', () => {
    usePanesStore.setState({
      everythingStatus: { status: 'available', isAvailable: true },
    })
    useConfigStore.setState({ autoFolderSize: true })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega', folderEntries(501))} isActive />)

    expect(screen.getByRole('button', { name: buttonName })).toBeInTheDocument()
  })

  it('keeps the button hidden at exactly 500 folders with auto sizing on', () => {
    usePanesStore.setState({
      everythingStatus: { status: 'available', isAvailable: true },
    })
    useConfigStore.setState({ autoFolderSize: true })

    render(<BreadcrumbBar pane={pane('C:\\Users\\Omega', folderEntries(500))} isActive />)

    expect(screen.queryByRole('button', { name: buttonName })).not.toBeInTheDocument()
  })
})
