import { act, fireEvent, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { ContextMenu } from '@/components/menus/ContextMenu'
import { commandContextAction, noopContextAction } from '@/lib/context-menu/context-menu-actions'
import { ipc } from '@/tests/ipc-mock'
import type { ContextMenuDocument } from '@/lib/types/context-menu'
import type { LoadNativeMenuRequest, LoadNativeMenuResponse } from '@/lib/types/ipc'
import { useClipboardStore } from '@/stores/clipboard-store'
import { useContextMenuStore } from '@/stores/context-menu-store'
import { useInlineRenameStore } from '@/stores/inline-rename-store'
import { usePanesStore } from '@/stores/panes-store'

const fileEntry = {
  id: 'report',
  name: 'Report.txt',
  path: 'C:\\Users\\Omega\\Report.txt',
  isDir: false,
  sizeBytes: 64,
  itemCount: null,
  typeLabel: 'TXT file',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

function openMenu(menu: Partial<ContextMenuDocument> = {}) {
  useContextMenuStore.getState().openMenu({
    paneId: 'left',
    x: 24,
    y: 48,
    title: 'Report.txt',
    topStrip: [],
    sections: [],
    nativeRequest: null,
    nativeSectionId: null,
    ...menu,
  })
}

function createDeferred<T>() {
  let resolve!: (value: T) => void
  let reject!: (error?: unknown) => void
  const promise = new Promise<T>((nextResolve, nextReject) => {
    resolve = nextResolve
    reject = nextReject
  })

  return { promise, resolve, reject }
}

describe('ContextMenu', () => {
  beforeEach(() => {
    ipc.install()
    useContextMenuStore.getState().closeMenu()
    useClipboardStore.getState().clearClipboard()
    useInlineRenameStore.getState().reset()
    usePanesStore.getState().reset()
    usePanesStore.setState({
      panes: {
        left: {
          ...usePanesStore.getState().panes.left,
          path: 'C:\\Users\\Omega',
          entries: [fileEntry],
          focusedEntryId: fileEntry.id,
        },
        right: usePanesStore.getState().panes.right,
      },
    })
  })

  it('renders quick actions, grouped rows, and starts keyboard activation on the first enabled strip action', () => {
    openMenu({
      topStrip: [
        {
          id: 'strip-hidden',
          label: 'Hidden action',
          owner: 'app',
          icon: { kind: 'app', name: 'cut' },
          hidden: true,
          action: noopContextAction('hidden'),
        },
        {
          id: 'strip-copy',
          label: 'Copy',
          owner: 'app',
          icon: { kind: 'app', name: 'copy' },
          action: commandContextAction('copy', fileEntry.id),
        },
      ],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'open',
              kind: 'action',
              label: 'Open',
              owner: 'app',
              shortcut: 'Enter',
              strong: true,
              action: noopContextAction('open'),
            },
          ],
        },
        {
          id: 'footer',
          rows: [
            {
              id: 'refresh',
              kind: 'action',
              label: 'Refresh',
              owner: 'app',
              action: noopContextAction('refresh'),
            },
          ],
        },
      ],
    })

    render(<ContextMenu />)

    const menu = screen.getByRole('menu', { name: 'Report.txt' })
    expect(screen.getByRole('menuitem', { name: 'Copy' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Hidden action' })).not.toBeInTheDocument()
    expect(screen.getByText('Open')).toBeInTheDocument()
    expect(screen.getByText('Enter')).toBeInTheDocument()

    fireEvent.keyDown(menu, { key: 'Enter' })

    expect(useClipboardStore.getState().entries).toEqual([fileEntry])
    expect(useContextMenuStore.getState().menu).toBeNull()
  })

  it('supports Home and End movement plus one-level submenu open and close state', () => {
    openMenu({
      topStrip: [
        {
          id: 'strip-copy',
          label: 'Copy',
          owner: 'app',
          icon: { kind: 'app', name: 'copy' },
          action: noopContextAction('copy'),
        },
      ],
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'open',
              kind: 'action',
              label: 'Open',
              owner: 'app',
              action: noopContextAction('open'),
            },
            {
              id: 'tools',
              kind: 'submenu',
              label: 'Tools',
              owner: 'app',
              children: {
                id: 'tools-panel',
                rows: [
                  {
                    id: 'tools-disabled',
                    label: 'Disabled tool',
                    owner: 'app',
                    disabled: true,
                    action: noopContextAction('disabled'),
                  },
                  {
                    id: 'tools-share',
                    label: 'Share placeholder',
                    owner: 'app',
                    action: noopContextAction('share'),
                  },
                ],
              },
            },
            {
              id: 'refresh',
              kind: 'action',
              label: 'Refresh',
              owner: 'app',
              action: noopContextAction('refresh'),
            },
          ],
        },
      ],
    })

    render(<ContextMenu />)
    const menu = screen.getByRole('menu', { name: 'Report.txt' })

    fireEvent.keyDown(menu, { key: 'End' })
    expect(useContextMenuStore.getState().activeItemId).toBe('refresh')

    fireEvent.keyDown(menu, { key: 'Home' })
    expect(useContextMenuStore.getState().activeItemId).toBe('strip-copy')

    fireEvent.keyDown(menu, { key: 'ArrowDown' })
    fireEvent.keyDown(menu, { key: 'ArrowDown' })
    expect(useContextMenuStore.getState().activeItemId).toBe('tools')

    fireEvent.keyDown(menu, { key: 'ArrowRight' })
    expect(useContextMenuStore.getState().openSubmenuId).toBe('tools')
    expect(useContextMenuStore.getState().activeItemId).toBe('tools-share')
    expect(screen.getByRole('menuitem', { name: 'Share placeholder' })).toBeInTheDocument()

    fireEvent.keyDown(menu, { key: 'ArrowLeft' })
    expect(useContextMenuStore.getState().openSubmenuId).toBeNull()
    expect(useContextMenuStore.getState().activeItemId).toBe('tools')
    expect(useContextMenuStore.getState().menu).not.toBeNull()

    fireEvent.keyDown(menu, { key: 'Escape' })
    expect(useContextMenuStore.getState().menu).toBeNull()
  })

  it('activates enabled items by click, ignores disabled clicks, and closes on backdrop press', async () => {
    const user = userEvent.setup()

    openMenu({
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'disabled',
              kind: 'action',
              label: 'Disabled item',
              owner: 'app',
              disabled: true,
              action: noopContextAction('disabled'),
            },
            {
              id: 'rename',
              kind: 'action',
              label: 'Rename',
              owner: 'app',
              action: commandContextAction('rename', fileEntry.id),
            },
          ],
        },
      ],
    })

    const { rerender } = render(<ContextMenu />)

    await user.click(screen.getByRole('menuitem', { name: 'Disabled item' }))
    expect(useInlineRenameStore.getState().rename).toBeNull()
    expect(useContextMenuStore.getState().menu).not.toBeNull()

    await user.click(screen.getByRole('menuitem', { name: 'Rename' }))
    expect(useInlineRenameStore.getState().rename?.entryId).toBe(fileEntry.id)
    expect(useContextMenuStore.getState().menu).toBeNull()

    act(() => {
      openMenu({
        sections: [
          {
            id: 'primary',
            rows: [
              {
                id: 'open',
                kind: 'action',
                label: 'Open',
                owner: 'app',
                action: noopContextAction('open'),
              },
            ],
          },
        ],
      })
    })
    rerender(<ContextMenu />)
    fireEvent.mouseDown(screen.getByRole('menu', { name: 'Report.txt' }).parentElement as HTMLElement)
    expect(useContextMenuStore.getState().menu).toBeNull()
  })

  it('activates top-strip Copy and Cut for the targeted row', async () => {
    const user = userEvent.setup()

    openMenu({
      topStrip: [
        {
          id: 'strip-copy',
          label: 'Copy',
          owner: 'app',
          icon: { kind: 'app', name: 'copy' },
          action: commandContextAction('copy', fileEntry.id),
        },
        {
          id: 'strip-cut',
          label: 'Cut',
          owner: 'app',
          icon: { kind: 'app', name: 'cut' },
          action: commandContextAction('cut', fileEntry.id),
        },
      ],
    })

    const view = render(<ContextMenu />)

    await user.click(screen.getByRole('menuitem', { name: 'Copy' }))
    expect(useClipboardStore.getState()).toMatchObject({
      mode: 'copy',
      sourcePaneId: 'left',
      entries: [{ id: fileEntry.id }],
    })

    act(() => {
      openMenu({
        topStrip: [
          {
            id: 'strip-copy',
            label: 'Copy',
            owner: 'app',
            icon: { kind: 'app', name: 'copy' },
            action: commandContextAction('copy', fileEntry.id),
          },
          {
            id: 'strip-cut',
            label: 'Cut',
            owner: 'app',
            icon: { kind: 'app', name: 'cut' },
            action: commandContextAction('cut', fileEntry.id),
          },
        ],
      })
    })
    view.rerender(<ContextMenu />)

    await user.click(screen.getByRole('menuitem', { name: 'Cut' }))
    expect(useClipboardStore.getState()).toMatchObject({
      mode: 'move',
      sourcePaneId: 'left',
      entries: [{ id: fileEntry.id }],
    })
  })

  it('opens one submenu by pointer and closes the menu before invoking a native child row', async () => {
    const user = userEvent.setup()
    const invokeNative = vi.fn(() => ({ handled: true, message: 'invoked:fixture' }))
    ipc.override('invoke_native_menu_action', invokeNative)

    openMenu({
      sections: [
        {
          id: 'native',
          rows: [
            {
              id: 'native-tools',
              kind: 'submenu',
              label: 'Native tools',
              owner: 'native',
              children: {
                id: 'native-tools-panel',
                rows: [
                  {
                    id: 'native-share',
                    label: 'Share with team',
                    owner: 'native',
                    action: { kind: 'invoke-native', token: 'native:req-1:2' },
                  },
                ],
              },
            },
            {
              id: 'native-leaf',
              kind: 'action',
              label: 'Open in Terminal',
              owner: 'native',
              action: { kind: 'invoke-native', token: 'native:req-1:3' },
            },
          ],
        },
      ],
    })

    render(<ContextMenu />)

    await user.hover(screen.getByRole('menuitem', { name: 'Native tools' }))
    expect(useContextMenuStore.getState().openSubmenuId).toBe('native-tools')

    await user.click(screen.getByRole('menuitem', { name: 'Share with team' }))

    expect(useContextMenuStore.getState().menu).toBeNull()
    expect(invokeNative).toHaveBeenCalledWith({ token: 'native:req-1:2' })
  })

  it('hydrates native rows asynchronously, dedupes app-owned overlaps, and invokes loaded leaf actions', async () => {
    const user = userEvent.setup()
    const invokeNative = vi.fn(() => ({ handled: true, message: 'invoked:fixture' }))
    ipc.override('invoke_native_menu_action', invokeNative)
    ipc.override('load_native_menu', (payload: LoadNativeMenuRequest) => ({
      requestId: payload.requestId,
      items: [
        {
          id: 'fixture-open-with',
          label: 'Open with Fixture',
          enabled: true,
          danger: false,
          canonicalActionKind: 'openWith',
          normalizedVerb: 'OpenWith',
          invokeToken: 'native:fixture-native-request:1',
          icon: null,
          children: [],
        },
        {
          id: 'fixture-archive-tools',
          label: 'Fixture archive tools',
          enabled: true,
          danger: false,
          canonicalActionKind: null,
          normalizedVerb: null,
          invokeToken: null,
          icon: null,
          children: [
            {
              id: 'fixture-compress',
              label: 'Add to fixture.zip',
              enabled: true,
              danger: false,
              canonicalActionKind: 'compress',
              normalizedVerb: 'compress',
              invokeToken: 'native:fixture-native-request:2',
              icon: null,
              children: [],
            },
            {
              id: 'fixture-share-with-team',
              label: 'Share with team',
              enabled: true,
              danger: false,
              canonicalActionKind: null,
              normalizedVerb: 'sharewithteam',
              invokeToken: 'native:fixture-native-request:4',
              icon: null,
              children: [],
            },
          ],
        },
        {
          id: 'fixture-open-terminal',
          label: 'Open in Fixture Terminal',
          enabled: true,
          danger: false,
          canonicalActionKind: null,
          normalizedVerb: 'openinfixtureterminal',
          invokeToken: 'native:fixture-native-request:5',
          icon: null,
          children: [],
        },
      ],
    }))

    openMenu({
      sections: [
        {
          id: 'primary',
          rows: [
            {
              id: 'open',
              kind: 'action',
              label: 'Open',
              owner: 'app',
              action: noopContextAction('open'),
            },
          ],
        },
        {
          id: 'footer',
          rows: [
            {
              id: 'properties',
              kind: 'action',
              label: 'Properties',
              owner: 'app',
              action: noopContextAction('properties'),
            },
          ],
        },
      ],
      nativeRequest: {
        targetKind: 'file',
        targetPath: fileEntry.path,
        folderPath: 'C:\\Users\\Omega',
        selectedPaths: [fileEntry.path],
      },
      nativeSectionId: 'native-extras',
    })

    render(<ContextMenu />)

    expect(await screen.findByRole('menuitem', { name: 'Open in Fixture Terminal' })).toBeInTheDocument()
    expect(screen.queryByRole('menuitem', { name: 'Open with Fixture' })).not.toBeInTheDocument()

    await user.hover(screen.getByRole('menuitem', { name: 'Fixture archive tools' }))
    expect(screen.getByRole('menuitem', { name: 'Share with team' })).toBeInTheDocument()

    await user.click(screen.getByRole('menuitem', { name: 'Open in Fixture Terminal' }))

    expect(useContextMenuStore.getState().menu).toBeNull()
    expect(invokeNative).toHaveBeenCalledWith({ token: 'native:fixture-native-request:5' })
  })

  it('keeps the native section reserved after the placeholder becomes visible and the load later fails', async () => {
    vi.useFakeTimers()
    const deferred = createDeferred<LoadNativeMenuResponse>()
    void deferred.promise.catch(() => undefined)
    ipc.override('load_native_menu', () => deferred.promise as never)

    try {
      openMenu({
        sections: [
          {
            id: 'primary',
            rows: [
              {
                id: 'open',
                kind: 'action',
                label: 'Open',
                owner: 'app',
                action: noopContextAction('open'),
              },
            ],
          },
          {
            id: 'footer',
            rows: [
              {
                id: 'properties',
                kind: 'action',
                label: 'Properties',
                owner: 'app',
                action: noopContextAction('properties'),
              },
            ],
          },
        ],
        nativeRequest: {
          targetKind: 'file',
          targetPath: fileEntry.path,
          folderPath: 'C:\\Users\\Omega',
          selectedPaths: [fileEntry.path],
        },
        nativeSectionId: 'native-extras',
      })

      const view = render(<ContextMenu />)

      expect(screen.queryByRole('status', { name: 'Loading native menu items' })).not.toBeInTheDocument()

      await act(async () => {
        vi.advanceTimersByTime(1000)
      })

      const status = screen.getByRole('status', { name: 'Loading native menu items' })
      expect(status).toBeInTheDocument()
      expect(status.parentElement?.className).not.toContain('overflow-y-auto')

      await act(async () => {
        deferred.reject(new Error('shell bridge failed'))
        await Promise.resolve()
      })

      expect(screen.queryByRole('status', { name: 'Loading native menu items' })).not.toBeInTheDocument()
      expect(screen.queryByRole('menuitem', { name: 'Open' })).toBeInTheDocument()
      expect(view.container.querySelector('div[aria-hidden="true"].min-h-24')).not.toBeNull()
    } finally {
      vi.useRealTimers()
    }
  })

  it('keeps native parents visible but disabled when children or invocation cannot be exposed', async () => {
    ipc.override('load_native_menu', (payload: LoadNativeMenuRequest) => ({
      requestId: payload.requestId,
      items: [
        {
          id: 'native-truncated-parent',
          label: 'Deep native submenu',
          enabled: true,
          danger: false,
          canonicalActionKind: null,
          normalizedVerb: null,
          invokeToken: null,
          icon: null,
          children: [],
        },
        {
          id: 'native-terminal',
          label: 'Open in Terminal',
          enabled: true,
          danger: false,
          canonicalActionKind: null,
          normalizedVerb: 'openinterminal',
          invokeToken: 'native:fixture-native-request:6',
          icon: null,
          children: [],
        },
      ],
    }))

    openMenu({
      sections: [{ id: 'footer', rows: [] }],
      nativeRequest: {
        targetKind: 'file',
        targetPath: fileEntry.path,
        folderPath: 'C:\\Users\\Omega',
        selectedPaths: [fileEntry.path],
      },
      nativeSectionId: 'native-extras',
    })

    render(<ContextMenu />)

    expect(await screen.findByRole('menuitem', { name: 'Open in Terminal' })).toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Deep native submenu' })).toBeDisabled()
  })

  it('clamps submenu panels into the viewport and falls back to the left side when needed', async () => {
    const user = userEvent.setup()
    const originalWidth = window.innerWidth
    const originalHeight = window.innerHeight
    const rectSpy = vi.spyOn(HTMLElement.prototype, 'getBoundingClientRect')

    Object.defineProperty(window, 'innerWidth', {
      configurable: true,
      writable: true,
      value: 320,
    })
    Object.defineProperty(window, 'innerHeight', {
      configurable: true,
      writable: true,
      value: 240,
    })

    rectSpy.mockImplementation(function mockRect(this: HTMLElement) {
      const role = this.getAttribute('role')
      const label = this.getAttribute('aria-label')

      if (role === 'menuitem' && this.textContent?.includes('Tools')) {
        return DOMRect.fromRect({ x: 220, y: 196, width: 80, height: 28 })
      }

      if (role === 'menu' && label === 'Tools') {
        return DOMRect.fromRect({ x: 0, y: 0, width: 256, height: 96 })
      }

      if (this.className === 'relative') {
        return DOMRect.fromRect({ x: 40, y: 20, width: 0, height: 0 })
      }

      return DOMRect.fromRect({ x: 0, y: 0, width: 0, height: 0 })
    })

    try {
      openMenu({
        sections: [
          {
            id: 'primary',
            rows: [
              {
                id: 'tools',
                kind: 'submenu',
                label: 'Tools',
                owner: 'app',
                children: {
                  id: 'tools-panel',
                  rows: [
                    {
                      id: 'tools-share',
                      label: 'Share placeholder',
                      owner: 'app',
                      action: noopContextAction('share'),
                    },
                  ],
                },
              },
            ],
          },
        ],
      })

      render(<ContextMenu />)
      await user.hover(screen.getByRole('menuitem', { name: 'Tools' }))

      const submenu = screen.getByRole('menu', { name: 'Tools' })
      expect(submenu.style.visibility).not.toBe('hidden')
      expect(submenu.style.left).toBe('-32px')
      expect(submenu.style.top).toBe('116px')
    } finally {
      rectSpy.mockRestore()
      Object.defineProperty(window, 'innerWidth', {
        configurable: true,
        writable: true,
        value: originalWidth,
      })
      Object.defineProperty(window, 'innerHeight', {
        configurable: true,
        writable: true,
        value: originalHeight,
      })
    }
  })

  it('ignores stale native responses after a newer menu replaces the previous request', async () => {
    const first = createDeferred<LoadNativeMenuResponse>()
    const second = createDeferred<LoadNativeMenuResponse>()
    const requestIds: string[] = []

    ipc.override('load_native_menu', (payload: LoadNativeMenuRequest) => {
      requestIds.push(payload.requestId)
      return (requestIds.length === 1 ? first.promise : second.promise) as never
    })

    openMenu({
      title: 'First item',
      sections: [{ id: 'footer', rows: [] }],
      nativeRequest: {
        targetKind: 'file',
        targetPath: 'C:\\Users\\Omega\\First.txt',
        folderPath: 'C:\\Users\\Omega',
        selectedPaths: ['C:\\Users\\Omega\\First.txt'],
      },
      nativeSectionId: 'native-extras',
    })

    const view = render(<ContextMenu />)

    act(() => {
      openMenu({
        title: 'Second item',
        sections: [{ id: 'footer', rows: [] }],
        nativeRequest: {
          targetKind: 'file',
          targetPath: 'C:\\Users\\Omega\\Second.txt',
          folderPath: 'C:\\Users\\Omega',
          selectedPaths: ['C:\\Users\\Omega\\Second.txt'],
        },
        nativeSectionId: 'native-extras',
      })
    })
    view.rerender(<ContextMenu />)

    await waitFor(() => {
      expect(requestIds).toHaveLength(2)
    })

    await act(async () => {
      second.resolve({
        requestId: requestIds[1] ?? 'native-request-missing-second',
        items: [
          {
            id: 'second-terminal',
            label: 'Open in Second Terminal',
            enabled: true,
            danger: false,
            canonicalActionKind: null,
            normalizedVerb: 'openinsecondterminal',
            invokeToken: 'native:second:1',
            icon: null,
            children: [],
          },
        ],
      })
      await Promise.resolve()
    })

    expect(await screen.findByRole('menuitem', { name: 'Open in Second Terminal' })).toBeInTheDocument()

    await act(async () => {
      first.resolve({
        requestId: requestIds[0] ?? 'native-request-missing-first',
        items: [
          {
            id: 'first-terminal',
            label: 'Open in First Terminal',
            enabled: true,
            danger: false,
            canonicalActionKind: null,
            normalizedVerb: 'openinfirstterminal',
            invokeToken: 'native:first:1',
            icon: null,
            children: [],
          },
        ],
      })
      await Promise.resolve()
    })

    expect(screen.queryByRole('menuitem', { name: 'Open in First Terminal' })).not.toBeInTheDocument()
    expect(screen.getByRole('menuitem', { name: 'Open in Second Terminal' })).toBeInTheDocument()
  })
})
