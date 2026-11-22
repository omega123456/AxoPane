import { describe, expect, it } from 'vitest'
import {
  clampIndex,
  planTabMove,
  withActiveTab,
  withAddedTab,
  withClonedTab,
  withClosedTab,
  withHydratedPane,
  withMovedTabs,
  withPatchedActiveTab,
  withTabLocked,
  type TabPanes,
  type TabState,
} from '@/stores/tabs/tab-transitions'

function tab(id: string, overrides: Partial<TabState> = {}): TabState {
  return {
    id,
    path: `/${id}`,
    sortKey: 'name',
    sortDirection: 'asc',
    filter: '',
    viewMode: 'details',
    locked: false,
    ...overrides,
  }
}

function panes(leftIds: string[], activeTabIndex = 0, rightIds = ['r1']): TabPanes {
  return {
    left: { activeTabIndex, tabs: leftIds.map((id) => tab(id)), previouslyActiveTabIds: [] },
    right: { activeTabIndex: 0, tabs: rightIds.map((id) => tab(id)), previouslyActiveTabIds: [] },
  }
}

const newId = () => 'new-id'

describe('tab-transitions', () => {
  it('clamps indexes into the tab range', () => {
    expect(clampIndex(5, 0)).toBe(0)
    expect(clampIndex(5, 3)).toBe(2)
    expect(clampIndex(-1, 3)).toBe(0)
  })

  it('reorders within a pane, following the active tab', () => {
    const move = planTabMove(
      panes(['a', 'b', 'c'], 0),
      { sourcePaneId: 'left', tabId: 'a', destinationPaneId: 'left', destinationIndex: 3 },
      newId,
    )
    expect(move?.result).toEqual({ kind: 'reorder', paneId: 'left', tabId: 'a' })
    expect(move?.panes.left?.tabs.map((item) => item.id)).toEqual(['b', 'c', 'a'])
    expect(move?.panes.left?.activeTabIndex).toBe(2)
  })

  it('rejects no-op, out-of-range and unknown moves', () => {
    const state = panes(['a', 'b'])
    const request = {
      sourcePaneId: 'left' as const,
      tabId: 'a',
      destinationPaneId: 'left' as const,
    }
    expect(planTabMove(state, { ...request, destinationIndex: 0 }, newId)).toBeNull()
    expect(planTabMove(state, { ...request, destinationIndex: 9 }, newId)).toBeNull()
    expect(planTabMove(state, { ...request, destinationIndex: 0.5 }, newId)).toBeNull()
    expect(planTabMove(state, { ...request, tabId: 'zz', destinationIndex: 1 }, newId)).toBeNull()
  })

  it('transfers between panes with a fresh id, never emptying the source', () => {
    const state = panes(['a', 'b'], 1)
    const move = planTabMove(
      state,
      { sourcePaneId: 'left', tabId: 'b', destinationPaneId: 'right', destinationIndex: 1 },
      newId,
    )
    expect(move?.result).toEqual({
      kind: 'transfer',
      sourcePaneId: 'left',
      destinationPaneId: 'right',
      sourceActiveChanged: true,
      destinationTabId: 'new-id',
    })
    const moved = withMovedTabs(state, move!)
    expect(moved.left).toEqual({ activeTabIndex: 0, tabs: [tab('a')], previouslyActiveTabIds: [] })
    expect(moved.right.tabs.map((item) => item.id)).toEqual(['r1', 'new-id'])
    expect(moved.right.activeTabIndex).toBe(1)
    const inactive = planTabMove(
      panes(['a', 'b'], 1),
      { sourcePaneId: 'left', tabId: 'a', destinationPaneId: 'right', destinationIndex: 0 },
      newId,
    )
    expect(inactive?.panes.left?.activeTabIndex).toBe(0)
    expect(
      planTabMove(
        panes(['a']),
        { sourcePaneId: 'left', tabId: 'a', destinationPaneId: 'right', destinationIndex: 0 },
        newId,
      ),
    ).toBeNull()
  })

  it('hydrates a pane, falling back when it has no tabs', () => {
    const fallback = [tab('fallback')]
    expect(
      withHydratedPane(panes(['a']), 'left', {
        pane: { activeTabIndex: 4, tabs: [tab('x'), tab('y')] },
        fallbackTabs: () => fallback,
      }).left.activeTabIndex,
    ).toBe(1)
    expect(
      withHydratedPane(panes(['a']), 'left', {
        pane: { activeTabIndex: 0, tabs: [] },
        fallbackTabs: () => fallback,
      }).left.tabs,
    ).toBe(fallback)
  })

  it('adds a tab, activating it unless told not to', () => {
    expect(
      withAddedTab(panes(['a']), 'left', { tab: tab('b'), activate: true }).left.activeTabIndex,
    ).toBe(1)
    expect(
      withAddedTab(panes(['a']), 'left', { tab: tab('b'), activate: false }).left.activeTabIndex,
    ).toBe(0)
  })

  it('closes tabs, shifting the active index and refusing locked or last tabs', () => {
    expect(withClosedTab(panes(['a', 'b', 'c'], 2), 'left', 'a')?.left.activeTabIndex).toBe(1)
    expect(withClosedTab(panes(['a', 'b', 'c'], 2), 'left', 'c')?.left.activeTabIndex).toBe(1)
    expect(withClosedTab(panes(['a', 'b', 'c'], 0), 'left', 'c')?.left.activeTabIndex).toBe(0)
    expect(withClosedTab(panes(['a']), 'left', 'a')).toBeNull()
    expect(withClosedTab(panes(['a', 'b']), 'left', 'zz')).toBeNull()
    const locked = withTabLocked(panes(['a', 'b']), 'left', { tabId: 'a', locked: true })
    expect(locked.left.tabs[0].locked).toBe(true)
    expect(withClosedTab(locked, 'left', 'a')).toBeNull()
  })

  it('returns to the previously active tab when the active tab closes', () => {
    const visitedCThenA = withActiveTab(
      withActiveTab(panes(['a', 'b', 'c', 'd'], 1), 'left', 'c')!,
      'left',
      'a',
    )!
    const afterClosingA = withClosedTab(visitedCThenA, 'left', 'a')!
    expect(afterClosingA.left.tabs[afterClosingA.left.activeTabIndex].id).toBe('c')
    const afterClosingC = withClosedTab(afterClosingA, 'left', 'c')!
    expect(afterClosingC.left.tabs[afterClosingC.left.activeTabIndex].id).toBe('b')
    const afterClosingB = withClosedTab(afterClosingC, 'left', 'b')!
    expect(afterClosingB.left.tabs[afterClosingB.left.activeTabIndex].id).toBe('d')
  })

  it('forgets closed tabs from the activation history', () => {
    const visitedBThenC = withActiveTab(
      withActiveTab(panes(['a', 'b', 'c'], 0), 'left', 'b')!,
      'left',
      'c',
    )!
    const withoutB = withClosedTab(visitedBThenC, 'left', 'b')!
    expect(withoutB.left.previouslyActiveTabIds).toEqual(['a'])
    const afterClosingC = withClosedTab(withoutB, 'left', 'c')!
    expect(afterClosingC.left.tabs[afterClosingC.left.activeTabIndex].id).toBe('a')
  })

  it('returns to the tab active before a newly added tab when it closes', () => {
    const opened = withAddedTab(panes(['a', 'b', 'c'], 0), 'left', {
      tab: tab('d'),
      activate: true,
    })
    const afterClosingD = withClosedTab(opened, 'left', 'd')!
    expect(afterClosingD.left.tabs[afterClosingD.left.activeTabIndex].id).toBe('a')
    const background = withAddedTab(panes(['a', 'b'], 1), 'left', {
      tab: tab('c'),
      activate: false,
    })
    expect(background.left.previouslyActiveTabIds).toEqual([])
  })

  it('returns the source pane to its previously active tab when the active tab moves away', () => {
    const visitedC = withActiveTab(panes(['a', 'b', 'c'], 0), 'left', 'c')!
    const move = planTabMove(
      visitedC,
      { sourcePaneId: 'left', tabId: 'c', destinationPaneId: 'right', destinationIndex: 0 },
      newId,
    )
    const moved = withMovedTabs(visitedC, move!)
    expect(moved.left.tabs[moved.left.activeTabIndex].id).toBe('a')
    expect(moved.right.previouslyActiveTabIds).toEqual(['r1'])
  })

  it('clones a tab next to the original, unlocked and active', () => {
    const state = withTabLocked(panes(['a', 'b', 'c'], 2), 'left', { tabId: 'a', locked: true })
    const cloned = withClonedTab(state, 'left', { tabId: 'a', cloneId: 'a-clone' })!
    expect(cloned.left.tabs.map((item) => item.id)).toEqual(['a', 'a-clone', 'b', 'c'])
    expect(cloned.left.tabs[1]).toEqual({ ...tab('a'), id: 'a-clone', locked: false })
    expect(cloned.left.activeTabIndex).toBe(1)
    const afterClosingClone = withClosedTab(cloned, 'left', 'a-clone')!
    expect(afterClosingClone.left.tabs[afterClosingClone.left.activeTabIndex].id).toBe('c')
    expect(withClonedTab(state, 'left', { tabId: 'zz', cloneId: 'zz-clone' })).toBeNull()
  })

  it('activates and patches the active tab', () => {
    expect(withActiveTab(panes(['a', 'b']), 'left', 'b')?.left.activeTabIndex).toBe(1)
    expect(withActiveTab(panes(['a', 'b']), 'left', 'a')).toBeNull()
    expect(withActiveTab(panes(['a', 'b']), 'left', 'zz')).toBeNull()
    const patched = withPatchedActiveTab(panes(['a', 'b'], 1), 'left', { filter: 'x' })
    expect(patched.left.tabs.map((item) => item.filter)).toEqual(['', 'x'])
  })
})
