import { beforeEach } from 'vitest'
import {
  activeTab,
  fromSessionPane,
  tabLabel,
  toSessionPane,
  useTabsStore,
} from '@/stores/tabs-store'

beforeEach(() => {
  useTabsStore.getState().reset()
})

describe('tabs-store', () => {
  it('adds and activates a new tab', () => {
    const store = useTabsStore.getState()
    const id = store.addTab('left', {
      path: 'C:\\a',
      sortKey: 'name',
      sortDirection: 'asc',
      filter: '',
    })

    const pane = useTabsStore.getState().panes.left
    expect(pane.tabs).toHaveLength(2)
    expect(pane.tabs[pane.activeTabIndex].id).toBe(id)
    expect(activeTab('left').path).toBe('C:\\a')
  })

  it('does not change the active tab when activate is false', () => {
    const store = useTabsStore.getState()
    store.addTab(
      'left',
      { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' },
      { activate: false },
    )
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })

  it('closes a tab and adjusts the active index', () => {
    const store = useTabsStore.getState()
    store.addTab('left', { path: 'C:\\a', sortKey: 'name', sortDirection: 'asc', filter: '' })
    const second = store.addTab('left', {
      path: 'C:\\b',
      sortKey: 'name',
      sortDirection: 'asc',
      filter: '',
    })

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

  it('guards locked tabs from store-level close routes and allows closing after unlock', () => {
    const store = useTabsStore.getState()
    const lockedId = store.addTab('left', {
      path: 'C:\\locked',
      sortKey: 'name',
      sortDirection: 'asc',
      filter: '',
      locked: true,
    })
    store.addTab('left', { path: 'C:\\other', sortKey: 'name', sortDirection: 'asc', filter: '' })
    store.closeTab('left', lockedId)
    expect(useTabsStore.getState().panes.left.tabs.some((tab) => tab.id === lockedId)).toBe(true)

    useTabsStore.getState().setTabLocked('left', lockedId, false)
    useTabsStore.getState().closeTab('left', lockedId)
    expect(useTabsStore.getState().panes.left.tabs.some((tab) => tab.id === lockedId)).toBe(false)
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
    useTabsStore.getState().patchActiveTab('left', { path: 'C:\\a\\deep', filter: 'x' })
    expect(activeTab('left').path).toBe('C:\\a\\deep')
    expect(activeTab('left').filter).toBe('x')
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
    expect(right.tabs[1].locked).toBe(false)
  })

  it('round-trips a locked flag and defaults legacy session tabs to unlocked', () => {
    const id = useTabsStore.getState().addTab('left', {
      path: 'C:\\locked',
      sortKey: 'name',
      sortDirection: 'asc',
      filter: '',
      locked: true,
    })
    expect(toSessionPane('left').tabs.find((tab) => tab.path === 'C:\\locked')?.locked).toBe(true)
    const legacy = fromSessionPane('right', {
      activeTabIndex: 0,
      tabs: [{ path: '/legacy', sortKey: 'name', sortDirection: 'asc', filter: '' } as never],
    })
    expect(legacy.tabs[0].locked).toBe(false)
    expect(useTabsStore.getState().panes.left.tabs.find((tab) => tab.id === id)?.locked).toBe(true)
  })

  it('derives a tab label from the trailing path segment', () => {
    expect(tabLabel({ path: 'C:\\a\\b\\Media' })).toBe('Media')
  })

  it('falls back to a single tab when hydrating an empty pane', () => {
    useTabsStore.getState().hydrate('left', { activeTabIndex: 5, tabs: [] })
    expect(useTabsStore.getState().panes.left.tabs).toHaveLength(1)
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })
})
