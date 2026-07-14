import { beforeEach } from 'vitest'
import {
  activeTab,
  fromSessionPane,
  tabLabel,
  toSessionPane,
  useTabsStore,
} from '@/stores/tabs-store'
import { useLayoutStore } from '@/stores/layout-store'

beforeEach(() => {
  useLayoutStore.getState().reset()
  useTabsStore.getState().reset()
})

describe('tabs-store', () => {
  it('adds and activates a new tab', () => {
    const store = useTabsStore.getState()
    const id = store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })

    const pane = useTabsStore.getState().panes.left
    expect(pane.tabs).toHaveLength(2)
    expect(pane.tabs[pane.activeTabIndex].id).toBe(id)
    expect(activeTab('left').path).toBe('C:\\a')
    expect(activeTab('left').viewMode).toBe('details')
  })

  it('does not change the active tab when activate is false', () => {
    const store = useTabsStore.getState()
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' }, { activate: false })
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })

  it('closes a tab and adjusts the active index', () => {
    const store = useTabsStore.getState()
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    const second = store.addTab('left', { path: 'C:\\b', sortKey: 'name', sortDirection: 'asc', filter: '' })

    // active is the third tab (index 2). Close it -> active falls back to index 1.
    useTabsStore.getState().closeTab('left', second)
    const pane = useTabsStore.getState().panes.left
    expect(pane.tabs).toHaveLength(2)
    expect(pane.activeTabIndex).toBe(1)
  })

  it('refuses to close the last remaining tab', () => {
    const onlyTab = useTabsStore.getState().panes.left.tabs[0].id
    useTabsStore.getState().closeTab('left', onlyTab)
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
  })

  it('shifts the active index left when an earlier tab is closed', () => {
    const store = useTabsStore.getState()
    const first = store.panes.left.tabs[0].id
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    // active is index 1. Closing index 0 should keep the same tab active (now index 0).
    useTabsStore.getState().closeTab('left', first)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
    expect(activeTab('left').path).toBe('C:\\a')
  })

  it('sets the active tab by id', () => {
    const store = useTabsStore.getState()
    const first = store.panes.left.tabs[0].id
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    useTabsStore.getState().setActiveTab('left', first)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })

  it('patches only the active tab', () => {
    const store = useTabsStore.getState()
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    useTabsStore.getState().patchActiveTab('left', {
      path: 'C:\\a\\deep',
      filter: 'x',
      viewMode: 'icons',
    })
    expect(activeTab('left').path).toBe('C:\\a\\deep')
    expect(activeTab('left').filter).toBe('x')
    expect(activeTab('left').viewMode).toBe('icons')
    expect(useTabsStore.getState().panes.left.tabs[0].path).toBe('.')
  })

  it('round-trips session panes', () => {
    const store = useTabsStore.getState()
    store.addTab('left', { path: 'C:\\a', sortKey: 'size', sortDirection: 'desc', filter: 'q' })
    const session = toSessionPane('left')
    expect(session.tabs).toHaveLength(2)
    expect(session.activeTabIndex).toBe(1)

    useTabsStore.getState().hydrate('right', fromSessionPane('right', session))
    const right = useTabsStore.getState().panes.right
    expect(right.tabs.map((tab) => tab.path)).toEqual(['.', 'C:\\a'])
    expect(right.tabs[1].sortKey).toBe('size')
    expect(right.tabs[1].viewMode).toBe('details')
  })

  it('keeps independent modes when switching active tabs', () => {
    const first = activeTab('left').id
    const second = useTabsStore
      .getState()
      .addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    useTabsStore.getState().patchActiveTab('left', { viewMode: 'thumbnails' })
    useTabsStore.getState().setActiveTab('left', first)
    useTabsStore.getState().patchActiveTab('left', { viewMode: 'icons' })

    expect(activeTab('left').viewMode).toBe('icons')
    useTabsStore.getState().setActiveTab('left', second)
    expect(activeTab('left').viewMode).toBe('thumbnails')
  })

  it('applies the configured default at every tab creation boundary', () => {
    useLayoutStore.getState().setDefaultViewMode('icons')
    useTabsStore.getState().reset()
    expect(activeTab('left').viewMode).toBe('icons')

    useTabsStore
      .getState()
      .addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    expect(activeTab('left').viewMode).toBe('icons')

    useTabsStore.getState().hydrate('right', { activeTabIndex: 0, tabs: [] })
    expect(activeTab('right').viewMode).toBe('icons')
  })

  it('round-trips explicit session modes and migrates missing or invalid modes', () => {
    useTabsStore.getState().patchActiveTab('left', { viewMode: 'thumbnails' })
    expect(toSessionPane('left').tabs[0]?.viewMode).toBe('thumbnails')

    useLayoutStore.getState().setDefaultViewMode('icons')
    const restored = fromSessionPane('right', {
      activeTabIndex: 0,
      tabs: [
        { path: 'C:\\old', sortKey: 'name', sortDirection: 'asc', filter: '' },
        { path: 'C:\\bad', sortKey: 'size', sortDirection: 'desc', filter: '', viewMode: 'legacy' },
        { path: 'C:\\saved', sortKey: 'modified', sortDirection: 'asc', filter: '', viewMode: 'details' },
      ],
    })

    expect(restored.tabs.map((tab) => tab.viewMode)).toEqual(['icons', 'icons', 'details'])
  })

  it('derives a tab label from the trailing path segment', () => {
    expect(tabLabel({ id: 't', path: 'C:\\a\\b\\Media', sortKey: 'name', sortDirection: 'asc', filter: '', viewMode: 'details' })).toBe('Media')
  })

  it('falls back to a single tab when hydrating an empty pane', () => {
    useTabsStore.getState().hydrate('left', { activeTabIndex: 5, tabs: [] })
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })
})
