import { render, screen } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, afterEach } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { FolderTree } from '@/components/tree/FolderTree'
import { TRASH_PATH } from '@/lib/trash'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useContextMenuStore.setState({ menu: null })
  setPlatform('Win32')
  usePanesStore.getState().initialize({
    session: { activePane: 'left', leftPath: '.', rightPath: '.' },
    showHiddenFiles: false,
    everythingStatus: { status: 'unavailable', isAvailable: false },
    volumes: [
      { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
    ],
  })
})

afterEach(() => {
  setPlatform(originalPlatform)
})

describe('TrashTreeRow', () => {
  it('renders "Recycle Bin" on Windows', () => {
    render(<FolderTree />)
    expect(screen.getByText('Recycle Bin')).toBeInTheDocument()
  })

  it('renders "Trash" on macOS', () => {
    setPlatform('MacIntel')
    render(<FolderTree />)
    expect(screen.getByText('Trash')).toBeInTheDocument()
  })

  it('navigates the active pane to the trash sentinel path on click', async () => {
    const user = userEvent.setup()
    render(<FolderTree />)

    await user.click(screen.getByText('Recycle Bin'))

    expect(usePanesStore.getState().panes.left.path).toBe(TRASH_PATH)
  })

  it('opens a context menu with an Empty Trash action on right-click', async () => {
    const user = userEvent.setup()
    render(<FolderTree />)

    await user.pointer({ keys: '[MouseRight]', target: screen.getByText('Recycle Bin') })

    const menu = useContextMenuStore.getState().menu
    expect(menu).not.toBeNull()
    const rows = menu?.sections.flatMap((section) => section.rows) ?? []
    expect(rows.some((row) => row.label === 'Empty Trash')).toBe(true)
  })
})
