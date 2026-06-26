import { fireEvent, render, screen, waitFor } from '@testing-library/react'
import { QueryClient, QueryClientProvider } from '@tanstack/react-query'
import { describe, expect, it, vi } from 'vitest'
import { ipc } from '../../ipc-mock'
import { fixtures } from '../../fixtures'
import { WorkspaceLayout } from '@/components/shell/WorkspaceLayout'
import type { PaneMode } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { useLayoutStore } from '@/stores/layout-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSelectionStore } from '@/stores/selection-store'
import { useTabsStore } from '@/stores/tabs-store'

function mockRect(element: Element, left: number, width: number) {
  element.getBoundingClientRect = () =>
    ({
      left,
      width,
      top: 0,
      right: left + width,
      bottom: 0,
      x: left,
      y: 0,
      height: 0,
      toJSON() {},
    }) as DOMRect
}

function renderLayout(mode: PaneMode = 'dual') {
  useConfigStore.getState().reset()
  useLayoutStore.getState().reset()
  usePanesStore.getState().reset()
  useTabsStore.getState().reset()
  useSelectionStore.getState().reset()
  useLayoutStore.setState({ defaultPaneMode: mode })

  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  })

  return render(
    <QueryClientProvider client={queryClient}>
      <WorkspaceLayout />
    </QueryClientProvider>,
  )
}

describe('WorkspaceLayout', () => {
  it('resizes the folder tree by the drag distance and persists the result', async () => {
    const saveConfig = vi.fn(() => fixtures.save_config)
    ipc.override('save_config', saveConfig)
    renderLayout()

    const handle = screen.getByRole('separator', { name: 'Resize folder tree' })

    // Grabbing away from the edge must not jump: only the movement delta applies.
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 260 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 306 })
    expect(useLayoutStore.getState().treeWidthPx).toBe(250)

    fireEvent.pointerUp(handle, { pointerId: 1 })
    await waitFor(() => expect(saveConfig).toHaveBeenCalled())
  })

  it('clamps the tree width to its maximum while dragging', () => {
    renderLayout()
    const handle = screen.getByRole('separator', { name: 'Resize folder tree' })

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 0 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 1500 })
    expect(useLayoutStore.getState().treeWidthPx).toBe(480)
  })

  it('resizes the pane split by the drag distance over the pane width', () => {
    renderLayout()
    const grid = document.querySelector('[data-pane-id="left"]')?.parentElement as Element
    mockRect(grid, 100, 800)
    const handle = screen.getByRole('separator', { name: 'Resize panes' })

    // Move 80px left across an 800px pane row → split shifts by 0.1 from 0.5.
    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 500 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 420 })
    expect(useLayoutStore.getState().paneSplit).toBeCloseTo(0.4)
  })

  it('keeps the split unchanged when the pane row has no measurable width', () => {
    renderLayout()
    const grid = document.querySelector('[data-pane-id="left"]')?.parentElement as Element
    mockRect(grid, 0, 0)
    const handle = screen.getByRole('separator', { name: 'Resize panes' })

    fireEvent.pointerDown(handle, { button: 0, pointerId: 1, clientX: 200 })
    fireEvent.pointerMove(handle, { pointerId: 1, clientX: 400 })
    expect(useLayoutStore.getState().paneSplit).toBe(0.5)
  })

  it('nudges the split with the keyboard', () => {
    renderLayout()
    const handle = screen.getByRole('separator', { name: 'Resize panes' })
    fireEvent.keyDown(handle, { key: 'ArrowRight' })
    expect(useLayoutStore.getState().paneSplit).toBeCloseTo(0.52)
  })

  it('renders a single pane without a pane divider in single mode', () => {
    renderLayout('single')

    expect(screen.queryByRole('separator', { name: 'Resize panes' })).not.toBeInTheDocument()
    expect(screen.getByRole('separator', { name: 'Resize folder tree' })).toBeInTheDocument()
  })
})
