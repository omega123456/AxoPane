import { render, screen } from '@testing-library/react'
import { beforeEach } from 'vitest'
import { ipc } from '@/tests/ipc-mock'
import { FolderTree } from '@/components/tree/FolderTree'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'

function treeRow(label: string) {
  const row = screen.getByText(label, { selector: 'span' }).closest('li')
  expect(row).not.toBeNull()
  return row
}

beforeEach(() => {
  ipc.install()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
})

function seedVolumes() {
  usePanesStore.getState().initialize({
    session: { activePane: 'left', leftPath: '.', rightPath: '.' },
    showHiddenFiles: false,
    everythingStatus: { status: 'unavailable', isAvailable: false },
    volumes: [
      { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
      { mountRoot: 'E:\\', label: 'USB Stick', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: true },
      { mountRoot: 'Z:\\', label: 'Share', totalBytes: 1, freeBytes: 1, isNetwork: true, isRemovable: false },
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
        { mountRoot: 'C:\\', label: 'Windows', totalBytes: 1, freeBytes: 1, isNetwork: false, isRemovable: false },
      ],
    })
    render(<FolderTree />)

    expect(screen.getByText('Drives')).toBeInTheDocument()
    expect(screen.queryByText('Removable Drives')).not.toBeInTheDocument()
    expect(screen.queryByText('Network Drives')).not.toBeInTheDocument()
  })

  it('pins the ancestor chain of the active node and leaves unrelated siblings unpinned', async () => {
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

    const rootRow = treeRow('Windows (C:)')
    const aaRow = treeRow('aa')
    const bbRow = treeRow('bb')
    const ccRow = treeRow('cc')

    expect(rootRow).toHaveStyle({ position: 'sticky' })
    expect(aaRow).toHaveStyle({ position: 'sticky' })
    expect(bbRow).toHaveStyle({ position: 'sticky' })
    expect(ccRow).not.toHaveStyle({ position: 'sticky' })

    expect(rootRow?.className).toContain('hover:bg-light-tree-hover')
    expect(aaRow?.className).toContain('hover:bg-light-tree-hover')
    expect(bbRow?.className).toContain('hover:bg-light-tree-current-hover')
    expect(ccRow?.className).toContain('hover:bg-light-hover')

    expect(Number(rootRow?.style.zIndex)).toBeLessThan(Number(aaRow?.style.zIndex))
    expect(Number(aaRow?.style.zIndex)).toBeLessThan(Number(bbRow?.style.zIndex))

    // The stack offset is a calc() against a fixed token, computed on the
    // very first render - not a value that only converges after each row's
    // own async height measurement (see regression below).
    expect(rootRow?.style.top).toBe('calc(var(--spacing-tree-row) * 0)')
    expect(aaRow?.style.top).toBe('calc(var(--spacing-tree-row) * 1)')
    expect(bbRow?.style.top).toBe('calc(var(--spacing-tree-row) * 2)')
  })

  it('renders visible volume rows as direct siblings in one flat list', async () => {
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

    render(<FolderTree />)
    await screen.findByText('bb', { selector: 'span' })

    const rootRow = treeRow('Windows (C:)')
    const volumeList = rootRow?.closest('ul')
    expect(volumeList).not.toBeNull()
    expect(Array.from(volumeList?.children ?? [])).toEqual([
      rootRow,
      treeRow('aa'),
      treeRow('bb'),
      treeRow('cc'),
    ])
    expect(volumeList?.querySelectorAll('ul')).toHaveLength(0)
  })

  it('gives every ancestor a distinct sticky offset when many levels are revealed in one jump', async () => {
    // Regression: navigating straight to a deeply nested path reveals every
    // ancestor's tree node in a single state update. Offsets must be correct
    // immediately (no render settling required), or deeper rows visually
    // collapse onto shallower ones.
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

    const names = ['a', 'b', 'c', 'd', 'e']
    const rows = names.map(treeRow)

    rows.forEach((row, index) => {
      expect(row?.style.top).toBe(`calc(var(--spacing-tree-row) * ${index + 1})`)
    })

    // Every level gets a strictly increasing z-index, so a deeper row never
    // renders visually on top of / merged with a shallower one.
    const zIndexes = rows.map((row) => Number(row?.style.zIndex))
    for (let i = 1; i < zIndexes.length; i += 1) {
      expect(zIndexes[i]).toBeGreaterThan(zIndexes[i - 1])
    }
  })
})
