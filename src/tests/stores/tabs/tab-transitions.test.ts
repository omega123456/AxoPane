import { describe, expect, it } from 'vitest'
import {
  clampIndex,
  planTabMove,
  withActiveTab,
  withAddedTab,
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
    left: { activeTabIndex, tabs: leftIds.map((id) => tab(id)) },
    right: { activeTabIndex: 0, tabs: rightIds.map((id) => tab(id)) },
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
    expect(moved.left).toEqual({ activeTabIndex: 0, tabs: [tab('a')] })
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
      withHydratedPane(
        panes(['a']),
        'left',
        { activeTabIndex: 4, tabs: [tab('x'), tab('y')] },
        () => fallback,
      ).left.activeTabIndex,
    ).toBe(1)
    expect(
      withHydratedPane(panes(['a']), 'left', { activeTabIndex: 0, tabs: [] }, () => fallback).left
        .tabs,
    ).toBe(fallback)
  })

  it('adds a tab, activating it unless told not to', () => {
    expect(withAddedTab(panes(['a']), 'left', tab('b'), true).left.activeTabIndex).toBe(1)
    expect(withAddedTab(panes(['a']), 'left', tab('b'), false).left.activeTabIndex).toBe(0)
  })

  it('closes tabs, shifting the active index and refusing locked or last tabs', () => {
    expect(withClosedTab(panes(['a', 'b', 'c'], 2), 'left', 'a')?.left.activeTabIndex).toBe(1)
    expect(withClosedTab(panes(['a', 'b', 'c'], 2), 'left', 'c')?.left.activeTabIndex).toBe(1)
    expect(withClosedTab(panes(['a', 'b', 'c'], 0), 'left', 'c')?.left.activeTabIndex).toBe(0)
    expect(withClosedTab(panes(['a']), 'left', 'a')).toBeNull()
    expect(withClosedTab(panes(['a', 'b']), 'left', 'zz')).toBeNull()
    const locked = withTabLocked(panes(['a', 'b']), 'left', 'a', true)
    expect(locked.left.tabs[0].locked).toBe(true)
    expect(withClosedTab(locked, 'left', 'a')).toBeNull()
  })

  it('activates and patches the active tab', () => {
    expect(withActiveTab(panes(['a', 'b']), 'left', 'b')?.left.activeTabIndex).toBe(1)
    expect(withActiveTab(panes(['a', 'b']), 'left', 'a')).toBeNull()
    expect(withActiveTab(panes(['a', 'b']), 'left', 'zz')).toBeNull()
    const patched = withPatchedActiveTab(panes(['a', 'b'], 1), 'left', { filter: 'x' })
    expect(patched.left.tabs.map((item) => item.filter)).toEqual(['', 'x'])
  })
})
