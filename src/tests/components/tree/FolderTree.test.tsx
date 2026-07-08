import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { FolderTree } from '@/components/tree/FolderTree'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useDragStore } from '@/stores/drag-store'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

function treeRow(label: string) {
  const row = screen.getByText(label, { selector: 'span' }).closest('[data-tree-row]')
  expect(row).not.toBeNull()
  return row as HTMLElement
}

/** Set the tree's scrollTop and dispatch a scroll event so the pinned overlay recomputes. */
function scrollTreeTo(scrollTop: number) {
  const scroll = screen.getByTestId('folder-tree-scroll')
  scroll.scrollTop = scrollTop
  fireEvent.scroll(scroll)
}

function pinnedRows() {
  return screen.queryAllByTestId('tree-pinned-row')
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useDragStore.getState().end()
  useContextMenuStore.setState({ menu: null })
})

afterEach(() => {
  setPlatform(originalPlatform)
})

function seedVolumes() {
  usePanesStore.getState().initialize({
    session: { activePane: 'left', leftPath: '.', rightPath: '.' },
    showHiddenFiles: false,
    everythingStatus: { status: 'unavailable', isAvailable: false },
    volumes: [
      {
        mountRoot: 'C:\\',
        label: 'Windows',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: false,
        isRemovable: false,
      },
      {
        mountRoot: 'E:\\',
        label: 'USB Stick',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: false,
        isRemovable: true,
      },
      {
        mountRoot: 'Z:\\',
        label: 'Share',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: true,
        isRemovable: false,
      },
    ],
  })
}

function seedMacVolumes() {
  usePanesStore.getState().initialize({
    session: { activePane: 'left', leftPath: '/', rightPath: '/' },
    showHiddenFiles: false,
    everythingStatus: { status: 'unavailable', isAvailable: false },
    volumes: [
      {
        mountRoot: '/',
        label: 'Macintosh HD',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: false,
        isRemovable: false,
      },
      {
        mountRoot: '/Volumes/Untitled',
        label: 'Untitled',
        totalBytes: 1,
        freeBytes: 1,
        isNetwork: false,
        isRemovable: true,
      },
    ],
  })
}

describe('FolderTree', () => {
  it('groups volumes under Drives, Removable Drives, and Network Drives headings', () => {
    seedVolumes()
    render(<FolderTree />)

    expect(screen.getByText('Drives')).toBeInTheDocument()
    expect(screen.getByText('Removable Drives')).toBeInTheDocument()
    expect(screen.getByText('Network Drives')).toBeInTheDocument()

    expect(screen.getByText('Windows (C:)')).toBeInTheDocument()
    expect(screen.getByText('USB Stick (E:)')).toBeInTheDocument()
    expect(screen.getByText('Share (Z:)')).toBeInTheDocument()
  })

  it('omits category headings that have no volumes', () => {
    usePanesStore.getState().initialize({
      session: { activePane: 'left', leftPath: '.', rightPath: '.' },
      showHiddenFiles: false,
      everythingStatus: { status: 'unavailable', isAvailable: false },
      volumes: [
        {
          mountRoot: 'C:\\',
          label: 'Windows',
          totalBytes: 1,
          freeBytes: 1,
          isNetwork: false,
          isRemovable: false,
        },
      ],
    })
    render(<FolderTree />)

    expect(screen.getByText('Drives')).toBeInTheDocument()
    expect(screen.queryByText('Removable Drives')).not.toBeInTheDocument()
    expect(screen.queryByText('Network Drives')).not.toBeInTheDocument()
  })

  it('pins the ancestor chain of the active node once scrolled past, leaving siblings unpinned', async () => {
    seedVolumes()
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\aa\\bb' } },
      treeNodes: {
        ...state.treeNodes,
        'C:\\': {
          ...state.treeNodes['C:\\'],
          children: ['C:\\aa', 'C:\\cc'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa': {
          id: 'C:\\aa',
          name: 'aa',
          path: 'C:\\aa',
          parentPath: 'C:\\',
          children: ['C:\\aa\\bb'],
          expanded: true,
          loaded: true,
        },
        'C:\\cc': {
          id: 'C:\\cc',
          name: 'cc',
          path: 'C:\\cc',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
        'C:\\aa\\bb': {
          id: 'C:\\aa\\bb',
          name: 'bb',
          path: 'C:\\aa\\bb',
          parentPath: 'C:\\aa',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    await screen.findByText('aa', { selector: 'span' })

    // Nothing is pinned until the chain is actually scrolled past.
    expect(pinnedRows()).toHaveLength(0)

    scrollTreeTo(420)

    const pinned = pinnedRows()
    expect(pinned.map((row) => row.textContent)).toEqual(['Windows (C:)', 'aa', 'bb'])

    // Unrelated sibling `cc` is never pinned.
    expect(pinned.some((row) => within(row).queryByText('cc'))).toBe(false)

    // Pinned rows stack from the viewport top: strictly increasing offset and
    // z-index so a deeper ancestor always paints above a shallower one.
    const tops = pinned.map((row) => Number.parseFloat(row.style.top))
    const zIndexes = pinned.map((row) => Number(row.style.zIndex))
    for (let i = 1; i < pinned.length; i += 1) {
      expect(tops[i]).toBeGreaterThan(tops[i - 1])
      expect(zIndexes[i]).toBeGreaterThan(zIndexes[i - 1])
    }

    // The pinned current row (bb) uses the opaque stand-in, not the translucent
    // selection tint, so rows scrolling underneath don't bleed through.
    const bbPinned = pinned[2]
    expect(within(bbPinned).getByText('bb').closest('[data-tree-row]')?.className).toContain(
      'bg-light-tree-current',
    )
  })

  it('renders every visible row in depth-first order, then the trash and other volumes', async () => {
    seedVolumes()
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\aa\\bb' } },
      treeNodes: {
        ...state.treeNodes,
        'C:\\': {
          ...state.treeNodes['C:\\'],
          children: ['C:\\aa', 'C:\\cc'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa': {
          id: 'C:\\aa',
          name: 'aa',
          path: 'C:\\aa',
          parentPath: 'C:\\',
          children: ['C:\\aa\\bb'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa\\bb': {
          id: 'C:\\aa\\bb',
          name: 'bb',
          path: 'C:\\aa\\bb',
          parentPath: 'C:\\aa',
          children: [],
          expanded: false,
          loaded: true,
        },
        'C:\\cc': {
          id: 'C:\\cc',
          name: 'cc',
          path: 'C:\\cc',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    const { container } = render(<FolderTree />)
    await screen.findByText('bb', { selector: 'span' })

    const order = Array.from(container.querySelectorAll('[data-tree-row]')).map((row) =>
      row.getAttribute('data-tree-row'),
    )
    expect(order).toEqual(['C:\\', 'C:\\aa', 'C:\\aa\\bb', 'C:\\cc', 'trash', 'E:\\', 'Z:\\'])
  })

  it('removes collapsed descendants and keeps the pinned overlay out of the macOS scrollbar lane', async () => {
    const user = userEvent.setup()
    setPlatform('MacIntel')
    seedVolumes()
    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        'C:\\': {
          ...state.treeNodes['C:\\'],
          children: ['C:\\aa', 'C:\\cc'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa': {
          id: 'C:\\aa',
          name: 'aa',
          path: 'C:\\aa',
          parentPath: 'C:\\',
          children: ['C:\\aa\\bb'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa\\bb': {
          id: 'C:\\aa\\bb',
          name: 'bb',
          path: 'C:\\aa\\bb',
          parentPath: 'C:\\aa',
          children: [],
          expanded: false,
          loaded: true,
        },
        'C:\\cc': {
          id: 'C:\\cc',
          name: 'cc',
          path: 'C:\\cc',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    expect(await screen.findByText('bb', { selector: 'span' })).toBeInTheDocument()

    const scroll = screen.getByTestId('folder-tree-scroll')
    expect(scroll).toHaveClass('overflow-x-hidden', 'overflow-y-auto', 'overscroll-contain')
    expect(treeRow('Windows (C:)').parentElement).toHaveClass('inset-x-0')

    await user.click(within(treeRow('Windows (C:)')).getByRole('button', { name: /Collapse/ }))

    await waitFor(() => {
      expect(screen.queryByText('aa', { selector: 'span' })).not.toBeInTheDocument()
      expect(screen.queryByText('bb', { selector: 'span' })).not.toBeInTheDocument()
    })
    expect(treeRow('USB Stick (E:)')).toBeInTheDocument()
  })

  it('does not leave /Volumes descendants painted over trash or removable rows on macOS', async () => {
    const user = userEvent.setup()
    setPlatform('MacIntel')
    seedMacVolumes()
    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        '/': {
          ...state.treeNodes['/'],
          children: ['/Volumes', '/var'],
          expanded: true,
          loaded: true,
        },
        '/Volumes': {
          id: '/Volumes',
          name: 'Volumes',
          path: '/Volumes',
          parentPath: '/',
          children: ['/Volumes/AxoPane', '/Volumes/Untitled'],
          expanded: false,
          loaded: true,
        },
        '/Volumes/AxoPane': {
          id: '/Volumes/AxoPane',
          name: 'AxoPane',
          path: '/Volumes/AxoPane',
          parentPath: '/Volumes',
          children: [],
          expanded: false,
          loaded: true,
        },
        '/Volumes/Untitled': {
          ...state.treeNodes['/Volumes/Untitled'],
          children: [],
          expanded: false,
          loaded: true,
        },
        '/var': {
          id: '/var',
          name: 'var',
          path: '/var',
          parentPath: '/',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    expect(screen.queryByText('AxoPane', { selector: 'span' })).not.toBeInTheDocument()

    await user.click(within(treeRow('Volumes')).getByRole('button', { name: /Expand/ }))

    expect(await screen.findByText('AxoPane', { selector: 'span' })).toBeInTheDocument()
    expect(screen.getAllByText('Untitled', { selector: 'span' })).toHaveLength(1)

    await user.click(within(treeRow('Volumes')).getByRole('button', { name: /Collapse/ }))

    await waitFor(() => {
      expect(screen.queryByText('AxoPane', { selector: 'span' })).not.toBeInTheDocument()
      expect(screen.getAllByText('Untitled', { selector: 'span' })).toHaveLength(1)
    })
    expect(treeRow('Trash').parentElement).toHaveClass('bg-light-tree', 'overflow-hidden')
    expect(treeRow('Untitled').parentElement).toHaveClass('bg-light-tree', 'overflow-hidden')
  })

  it('keeps pinned rows out of the macOS overlay-scrollbar lane', async () => {
    setPlatform('MacIntel')
    seedVolumes()
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: 'C:\\aa' } },
      treeNodes: {
        ...state.treeNodes,
        'C:\\': {
          ...state.treeNodes['C:\\'],
          children: ['C:\\aa'],
          expanded: true,
          loaded: true,
        },
        'C:\\aa': {
          id: 'C:\\aa',
          name: 'aa',
          path: 'C:\\aa',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    await screen.findByText('aa', { selector: 'span' })
    scrollTreeTo(420)

    const pinnedOverlay = pinnedRows()[0]?.parentElement
    expect(pinnedOverlay).toHaveClass('right-2')
  })

  it('gives every ancestor a distinct pinned slot when many levels are revealed in one jump', async () => {
    // Regression: navigating straight to a deeply nested path reveals every
    // ancestor's tree node in a single state update. Once scrolled past, each
    // pins to its own slot with a strictly increasing offset and z-index so
    // deeper rows never visually collapse onto shallower ones.
    seedVolumes()
    const deepPath = 'C:\\a\\b\\c\\d\\e'
    usePanesStore.setState((state) => ({
      panes: { ...state.panes, left: { ...state.panes.left, path: deepPath } },
      treeNodes: {
        ...state.treeNodes,
        'C:\\': { ...state.treeNodes['C:\\'], children: ['C:\\a'], expanded: true, loaded: true },
        'C:\\a': {
          id: 'C:\\a',
          name: 'a',
          path: 'C:\\a',
          parentPath: 'C:\\',
          children: ['C:\\a\\b'],
          expanded: true,
          loaded: true,
        },
        'C:\\a\\b': {
          id: 'C:\\a\\b',
          name: 'b',
          path: 'C:\\a\\b',
          parentPath: 'C:\\a',
          children: ['C:\\a\\b\\c'],
          expanded: true,
          loaded: true,
        },
        'C:\\a\\b\\c': {
          id: 'C:\\a\\b\\c',
          name: 'c',
          path: 'C:\\a\\b\\c',
          parentPath: 'C:\\a\\b',
          children: ['C:\\a\\b\\c\\d'],
          expanded: true,
          loaded: true,
        },
        'C:\\a\\b\\c\\d': {
          id: 'C:\\a\\b\\c\\d',
          name: 'd',
          path: 'C:\\a\\b\\c\\d',
          parentPath: 'C:\\a\\b\\c',
          children: ['C:\\a\\b\\c\\d\\e'],
          expanded: true,
          loaded: true,
        },
        'C:\\a\\b\\c\\d\\e': {
          id: deepPath,
          name: 'e',
          path: deepPath,
          parentPath: 'C:\\a\\b\\c\\d',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    await screen.findByText('e', { selector: 'span' })

    scrollTreeTo(420)

    // The whole chain (volume root + a..e) is pinned.
    const pinned = pinnedRows()
    expect(pinned).toHaveLength(6)

    const tops = pinned.map((row) => Number.parseFloat(row.style.top))
    const zIndexes = pinned.map((row) => Number(row.style.zIndex))
    for (let i = 1; i < pinned.length; i += 1) {
      expect(tops[i]).toBeGreaterThan(tops[i - 1])
      expect(zIndexes[i]).toBeGreaterThan(zIndexes[i - 1])
    }
  })

  it('wires each node row to the store: toggle, navigate, open-tab, and context menu', async () => {
    const user = userEvent.setup()
    const toggle = vi.fn(() => Promise.resolve())
    const navigate = vi.fn(() => Promise.resolve())
    const openTab = vi.fn(() => Promise.resolve())
    seedVolumes()
    usePanesStore.setState((state) => ({
      toggleTreeNode: toggle,
      navigatePane: navigate,
      openTabFromPath: openTab,
      treeNodes: {
        ...state.treeNodes,
        'C:\\': { ...state.treeNodes['C:\\'], children: ['C:\\cc'], expanded: true, loaded: true },
        'C:\\cc': {
          id: 'C:\\cc',
          name: 'cc',
          path: 'C:\\cc',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))

    render(<FolderTree />)
    const ccRow = treeRow('cc')

    await user.click(within(ccRow).getByRole('button', { name: /Expand cc/ }))
    expect(toggle).toHaveBeenCalledWith('C:\\cc')

    await user.click(screen.getByText('cc'))
    expect(navigate).toHaveBeenCalledWith('left', 'C:\\cc')

    await user.pointer({ keys: '[MouseMiddle]', target: screen.getByText('cc') })
    expect(openTab).toHaveBeenCalledWith('left', 'C:\\cc')

    fireEvent.contextMenu(ccRow)
    expect(useContextMenuStore.getState().menu?.title).toBe('cc')
  })

  it('surfaces Eject for a removable drive row on macOS but not for fixed or network rows', () => {
    setPlatform('MacIntel')
    seedVolumes()
    render(
      <>
        <FolderTree />
        <ContextMenu />
      </>,
    )

    fireEvent.contextMenu(treeRow('Windows (C:)'))
    expect(screen.queryByRole('menuitem', { name: 'Eject' })).not.toBeInTheDocument()

    fireEvent.contextMenu(treeRow('Share (Z:)'))
    expect(screen.queryByRole('menuitem', { name: 'Eject' })).not.toBeInTheDocument()

    fireEvent.contextMenu(treeRow('USB Stick (E:)'))
    expect(screen.getByRole('menuitem', { name: 'Eject' })).toBeInTheDocument()
  })

  it('does not surface Eject on Windows (native shell Eject is used instead)', () => {
    setPlatform('Win32')
    seedVolumes()
    render(
      <>
        <FolderTree />
        <ContextMenu />
      </>,
    )

    fireEvent.contextMenu(treeRow('USB Stick (E:)'))
    expect(screen.queryByRole('menuitem', { name: 'Eject' })).not.toBeInTheDocument()
  })

  it('accepts an internal drop onto a folder node and enqueues the transfer', async () => {
    const startOp = vi.fn(() => 'op-1')
    ipc.override('start_op', startOp)
    seedVolumes()
    usePanesStore.setState((state) => ({
      treeNodes: {
        ...state.treeNodes,
        'C:\\': { ...state.treeNodes['C:\\'], children: ['C:\\cc'], expanded: true, loaded: true },
        'C:\\cc': {
          id: 'C:\\cc',
          name: 'cc',
          path: 'C:\\cc',
          parentPath: 'C:\\',
          children: [],
          expanded: false,
          loaded: true,
        },
      },
    }))
    // A drag originating from a different folder on the same volume.
    useDragStore.getState().begin({
      sourcePaneId: 'left',
      sourceDir: 'C:\\root',
      items: [{ id: 'a', name: 'Alpha', path: 'C:\\root\\Alpha', isDir: false, sizeBytes: 10 }],
    })

    render(<FolderTree />)
    const row = treeRow('cc')

    fireEvent.dragOver(row, { dataTransfer: { dropEffect: '' } })
    expect(row).toHaveClass('ring-accent-blue-border')

    fireEvent.drop(row, { dataTransfer: { dropEffect: '' } })
    await waitFor(() => {
      expect(startOp).toHaveBeenCalledWith({
        kind: 'move',
        destinationDir: 'C:\\cc',
        items: [{ sourcePath: 'C:\\root\\Alpha', name: 'Alpha', sizeBytes: 10 }],
      })
    })
    expect(useDragStore.getState().drag).toBeNull()
  })
})
