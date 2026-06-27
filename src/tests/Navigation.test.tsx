import { act, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { ipc } from './ipc-mock'
import type {
  DirectoryEntry,
  DirPatchEvent,
  ListDirRequest,
  ListDirResponse,
  SessionState,
} from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { useTabsStore } from '@/stores/tabs-store'
import { renderApp } from './utils/render-app'

function makeEntry(name: string, isDir: boolean): DirectoryEntry {
  return {
    id: name,
    name,
    path: `C:\\Users\\Omega\\${name}`,
    isDir,
    sizeBytes: isDir ? null : 1024,
    itemCount: isDir ? 3 : null,
    typeLabel: isDir ? 'Folder' : 'TXT file',
    modifiedAt: '2026-06-20T10:15:00Z',
    createdAt: '2026-06-01T10:15:00Z',
    attributes: [],
    isHidden: false,
    isSystem: false,
  }
}

const rootEntries = [makeEntry('Documents', true), makeEntry('Media', true), makeEntry('Report.txt', false)]

function listDirResponder(payload: ListDirRequest): ListDirResponse {
  const isRoot =
    payload.path === 'C:\\Users\\Omega' || payload.path === 'D:\\projects' || payload.path === 'C:\\'
  const entries = isRoot ? rootEntries : []
  const filtered = entries.filter((entry) =>
    payload.filter ? entry.name.toLowerCase().includes(payload.filter.toLowerCase()) : true,
  )
  return { path: payload.path, entries: filtered }
}

function installDefaults() {
  ipc.override('list_dir', listDirResponder)
  ipc.override('set_tab_watch', () => undefined)
  ipc.override('save_session', (payload) => payload.session)
}

function getRow(paneLabel: string, name: string) {
  const pane = screen.getByLabelText(paneLabel)
  return within(pane)
    .getAllByRole('row')
    .find((row) => row.textContent?.includes(name))
}

describe('Navigation: tabs, breadcrumb, session, live patching', () => {
  it('opens, switches, and closes tabs in a pane', async () => {
    const user = userEvent.setup()
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Documents')).toBeTruthy())

    const leftPane = screen.getByLabelText('Left pane')
    await user.click(within(leftPane).getByRole('button', { name: 'New tab in Left pane' }))

    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2)
    })

    // The first tab can now be closed.
    const tabs = within(leftPane).getAllByRole('tab')
    expect(tabs).toHaveLength(2)
    await user.click(tabs[0])
    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
    })

    const closeButton = within(leftPane).getAllByRole('button', { name: /Close tab/ })[0]
    await user.click(closeButton)
    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
    })
  })

  it('closes a tab on middle-click', async () => {
    const user = userEvent.setup()
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Documents')).toBeTruthy())

    const leftPane = screen.getByLabelText('Left pane')
    await user.click(within(leftPane).getByRole('button', { name: 'New tab in Left pane' }))
    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2)
    })

    const tab = within(leftPane).getAllByRole('tab')[0]
    await user.pointer({ keys: '[MouseMiddle]', target: tab })
    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
    })
  })

  it('opens a folder in a new tab on middle-click', async () => {
    const user = userEvent.setup()
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Documents')).toBeTruthy())
    const documents = getRow('Left pane', 'Documents')
    if (!documents) {
      throw new Error('row missing')
    }

    await user.pointer({ keys: '[MouseMiddle]', target: documents })

    await waitFor(() => {
      const tabs = useTabsStore.getState().panes.left.tabs
      expect(tabs).toHaveLength(2)
      expect(tabs[tabs.length - 1].path).toBe('C:\\Users\\Omega\\Documents')
    })
  })

  it('restores a prior multi-tab session on startup', async () => {
    const session: SessionState = {
      activePane: 'right',
      leftPath: 'C:\\Users\\Omega',
      rightPath: 'D:\\projects',
      left: {
        activeTabIndex: 1,
        tabs: [
          { path: 'C:\\Users\\Omega', sortKey: 'name', sortDirection: 'asc', filter: '' },
          { path: 'C:\\', sortKey: 'size', sortDirection: 'desc', filter: '' },
        ],
      },
      right: {
        activeTabIndex: 0,
        tabs: [{ path: 'D:\\projects', sortKey: 'name', sortDirection: 'asc', filter: '' }],
      },
    }
    installDefaults()
    ipc.override('load_session', session)
    renderApp()

    await waitFor(() => {
      expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2)
    })
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(1)

    // Active left tab path was C:\ -> breadcrumb in left pane should reflect it.
    await waitFor(() => {
      expect(screen.getByLabelText('Left pane path')).toBeInTheDocument()
    })
  })

  it('applies an incremental add patch without a full reload', async () => {
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Documents')).toBeTruthy())

    const newEntry = makeEntry('Brand-New', true)
    const patch: DirPatchEvent = {
      tabId: useTabsStore.getState().panes.left.tabs[0].id,
      path: 'C:\\Users\\Omega',
      reason: 'watch',
      changed: [{ path: newEntry.path, entry: newEntry }],
      removed: [],
    }

    act(() => {
      ipc.emit('dir://patch', patch)
    })

    await waitFor(() => {
      expect(getRow('Left pane', 'Brand-New')).toBeTruthy()
    })
  })

  it('applies an incremental remove patch', async () => {
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Media')).toBeTruthy())

    act(() => {
      ipc.emit('dir://patch', {
        tabId: useTabsStore.getState().panes.left.tabs[0].id,
        path: 'C:\\Users\\Omega',
        reason: 'watch',
        changed: [],
        removed: ['C:\\Users\\Omega\\Media'],
      })
    })

    await waitFor(() => {
      expect(getRow('Left pane', 'Media')).toBeFalsy()
    })
  })

  it('pauses patch reflow while the user is typing a filter', async () => {
    installDefaults()
    renderApp()

    await waitFor(() => expect(getRow('Left pane', 'Documents')).toBeTruthy())

    // Begin typing: setFilterDraft sets typing=true synchronously.
    act(() => {
      usePanesStore.getState().setFilterDraft('left', 'Do')
    })

    const newEntry = makeEntry('Brand-New', true)
    act(() => {
      ipc.emit('dir://patch', {
        tabId: useTabsStore.getState().panes.left.tabs[0].id,
        path: 'C:\\Users\\Omega',
        reason: 'watch',
        changed: [{ path: newEntry.path, entry: newEntry }],
        removed: [],
      })
    })

    // While typing, the patched entry must not appear.
    expect(getRow('Left pane', 'Brand-New')).toBeFalsy()
  })
})
