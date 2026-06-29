import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { BreadcrumbBar, splitPath } from '@/components/pane/BreadcrumbBar'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import type { PaneState } from '@/types/pane'

function pane(path: string): PaneState {
  return {
    id: 'left',
    title: 'Left pane',
    path,
    entries: [],
    focusedEntryId: null,
    sortKey: 'name',
    sortDirection: 'asc',
    filterDraft: '',
    filterApplied: '',
    typing: false,
    loading: false,
    error: null,
    visibleStartIndex: 0,
    visibleEndIndex: 40,
    scrollPositions: {},
  }
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
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
})
