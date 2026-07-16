import { act, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { useTabsStore } from '@/stores/tabs-store'
import { TabDragDropProvider } from '@/components/pane/TabDragDropProvider'
import { renderApp } from '../../utils/render-app'

const dnd = vi.hoisted(() => ({ handlers: {} as Record<string, (event: unknown) => void> }))

vi.mock('@dnd-kit/helpers', () => ({
  move: (_tabIds: unknown, event: { operation: { projected: unknown } }) =>
    event.operation.projected,
}))

vi.mock('@dnd-kit/react', async () => {
  const React = await import('react')
  return {
    DragDropProvider: ({ children, ...handlers }: { children: React.ReactNode }) => {
      dnd.handlers = handlers as Record<string, (event: unknown) => void>
      return React.createElement(React.Fragment, null, children)
    },
    PointerSensor: { configure: () => ({}) },
    useDroppable: () => ({ ref: () => {} }),
  }
})

vi.mock('@dnd-kit/react/sortable', () => ({
  useSortable: () => ({ handleRef: () => {}, isDragSource: false, ref: () => {} }),
}))

function operation(paneId: 'left' | 'right', id: string) {
  return { data: { paneId }, group: paneId, id }
}

function dragEvent(
  sourcePaneId: 'left' | 'right',
  sourceTabId: string,
  targetPaneId: 'left' | 'right' | null,
  projected: { left: string[]; right: string[] },
  canceled = false,
) {
  return {
    canceled,
    operation: {
      projected,
      source: operation(sourcePaneId, sourceTabId),
      target: targetPaneId ? operation(targetPaneId, `target-${targetPaneId}`) : null,
    },
    preventDefault: vi.fn(),
    suspend: vi.fn(() => ({ resume: vi.fn() })),
  }
}

async function addLeftTab() {
  const user = userEvent.setup()
  const leftPane = await screen.findByRole('region', { name: 'Left pane' })
  await user.click(within(leftPane).getByRole('button', { name: 'New tab in Left pane' }))
  await waitFor(() => expect(useTabsStore.getState().panes.left.tabs).toHaveLength(2))
}

describe('TabDragDropProvider', () => {
  beforeEach(() => {
    dnd.handlers = {}
  })

  it('keeps a short primary-pointer gesture as a normal tab click', async () => {
    const user = userEvent.setup()
    renderApp()
    await addLeftTab()

    const leftPane = await screen.findByRole('region', { name: 'Left pane' })
    const tabs = within(leftPane).getAllByRole('tab')
    await user.click(tabs[0])
    expect(useTabsStore.getState().panes.left.activeTabIndex).toBe(0)
  })

  it('previews then commits a valid same-pane move without changing tabs during hover', async () => {
    renderApp()
    await addLeftTab()
    const [firstTab, secondTab] = useTabsStore.getState().panes.left.tabs
    const projected = {
      left: [secondTab.id, firstTab.id],
      right: useTabsStore.getState().panes.right.tabs.map((tab) => tab.id),
    }

    await act(async () =>
      dnd.handlers.onDragStart(dragEvent('left', firstTab.id, 'left', projected)),
    )
    await act(async () =>
      dnd.handlers.onDragOver(dragEvent('left', firstTab.id, 'left', projected)),
    )
    expect(useTabsStore.getState().panes.left.tabs.map((tab) => tab.id)).toEqual([
      firstTab.id,
      secondTab.id,
    ])

    await act(async () => dnd.handlers.onDragEnd(dragEvent('left', firstTab.id, 'left', projected)))
    await waitFor(() =>
      expect(useTabsStore.getState().panes.left.tabs.map((tab) => tab.id)).toEqual(projected.left),
    )
  })

  it('rejects a last-tab cross-pane target and leaves canonical tabs unchanged', async () => {
    act(() => {
      useTabsStore.getState().reset()
      render(
        <TabDragDropProvider>
          <div />
        </TabDragDropProvider>,
      )
    })
    const sourceTab = useTabsStore.getState().panes.left.tabs[0]
    const before = structuredClone(useTabsStore.getState().panes)
    const event = dragEvent('left', sourceTab.id, 'right', {
      left: [],
      right: [...before.right.tabs.map((tab) => tab.id), sourceTab.id],
    })

    act(() => dnd.handlers.onDragOver(event))
    expect(event.preventDefault).toHaveBeenCalledOnce()

    act(() => dnd.handlers.onDragEnd(event))
    expect(useTabsStore.getState().panes.left.tabs).toEqual(before.left.tabs)
    expect(useTabsStore.getState().panes.right.tabs).toEqual(before.right.tabs)
  })

  it('restores the source label focus after an invalid release', async () => {
    renderApp()
    const source = await within(
      await screen.findByRole('region', { name: 'Left pane' }),
    ).findByRole('tab')
    const sourceTabId = source.dataset.tabLabelId
    expect(sourceTabId).toBeDefined()
    source.focus()
    const event = dragEvent('left', sourceTabId!, 'right', {
      left: [],
      right: [...useTabsStore.getState().panes.right.tabs.map((tab) => tab.id), sourceTabId!],
    })

    await act(async () => dnd.handlers.onDragStart(event))
    await act(async () => dnd.handlers.onDragOver(event))
    await act(async () => dnd.handlers.onDragEnd(event))

    expect(source).toHaveFocus()
  })

  it('cancels an outside drop without changing tabs', async () => {
    renderApp()
    await addLeftTab()
    const sourceTab = useTabsStore.getState().panes.left.tabs[0]
    const before = useTabsStore.getState().panes.left.tabs
    const source = document.querySelector<HTMLElement>(`[data-tab-label-id="${sourceTab.id}"]`)
    expect(source).not.toBeNull()
    source?.focus()

    await act(async () =>
      dnd.handlers.onDragStart(
        dragEvent('left', sourceTab.id, 'left', { left: before.map((tab) => tab.id), right: [] }),
      ),
    )
    await act(async () =>
      dnd.handlers.onDragEnd(
        dragEvent(
          'left',
          sourceTab.id,
          null,
          { left: before.map((tab) => tab.id), right: [] },
          true,
        ),
      ),
    )
    expect(useTabsStore.getState().panes.left.tabs).toEqual(before)
    expect(source).toHaveFocus()
  })

  it('restores the React-owned tab before committing a cross-pane optimistic move', async () => {
    renderApp()
    await addLeftTab()
    const sourceTab = useTabsStore.getState().panes.left.tabs[0]
    const rightTabIds = useTabsStore.getState().panes.right.tabs.map((tab) => tab.id)
    const projected = {
      left: [useTabsStore.getState().panes.left.tabs[1].id],
      right: [...rightTabIds, sourceTab.id],
    }

    await act(async () =>
      dnd.handlers.onDragStart(dragEvent('left', sourceTab.id, 'right', projected)),
    )
    const dragOverEvent = dragEvent('left', sourceTab.id, 'right', projected)
    await act(async () => dnd.handlers.onDragOver(dragOverEvent))
    expect(dragOverEvent.preventDefault).not.toHaveBeenCalled()
    const sourceElement = document.querySelector<HTMLElement>(`[data-tab-id="${sourceTab.id}"]`)
    const rightStrip = document.querySelector('[data-tab-strip="right"]')
    const placeholder = document.createElement('div')
    placeholder.dataset.dndPlaceholder = 'hidden'
    rightStrip?.append(sourceElement!, placeholder)

    const event = dragEvent('left', sourceTab.id, 'right', projected)
    await act(async () => dnd.handlers.onDragEnd(event))
    expect(event.suspend).not.toHaveBeenCalled()
    expect(useTabsStore.getState().panes.right.tabs).toHaveLength(2)
    const destinationTabId = useTabsStore.getState().panes.right.tabs[1].id
    await waitFor(() =>
      expect(document.querySelector(`[data-tab-label-id="${destinationTabId}"]`)).toHaveFocus(),
    )
  })
})
