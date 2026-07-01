import { act, render, screen, waitFor } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, vi } from 'vitest'
import { DefaultAppDialog } from '@/components/dialogs/DefaultAppDialog'
import { PropertiesDialog } from '@/components/dialogs/PropertiesDialog'
import { useConfigStore } from '@/stores/config-store'
import { useDefaultAppDialogStore } from '@/stores/default-app-dialog-store'
import { usePropertiesDialogStore } from '@/stores/properties-dialog-store'
import { ipc } from '@/tests/ipc-mock'
import type { ListApplicationsResponse } from '@/lib/types/ipc'

function createDeferred<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve
  })
  return { promise, resolve }
}

const originalPlatform = navigator.platform

function setPlatform(value: string) {
  Object.defineProperty(navigator, 'platform', { value, configurable: true })
}

beforeEach(() => {
  usePropertiesDialogStore.getState().close()
  useDefaultAppDialogStore.getState().close()
  useConfigStore.getState().reset()
})

afterEach(() => {
  vi.useRealTimers()
  setPlatform(originalPlatform)
})

describe('PropertiesDialog', () => {
  it('renders single-item properties and closes on button click', async () => {
    const user = userEvent.setup()
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'report',
          name: 'Report.txt',
          path: 'C:\\root\\Report.txt',
          isDir: false,
          sizeBytes: 2048,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: '2026-06-20T10:15:00Z',
          createdAt: '2026-06-01T10:15:00Z',
          attributes: ['Archive'],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('Report.txt')).toBeInTheDocument()
    expect(screen.getByText('2.0 KB')).toBeInTheDocument()
    expect(screen.getByText('TXT file')).toBeInTheDocument()
    expect(screen.getAllByText('—').length).toBeGreaterThan(0)

    await user.click(screen.getByRole('button', { name: 'Close' }))
    expect(usePropertiesDialogStore.getState().dialog).toBeNull()
  })

  it('renders multi-selection fallback content and closes on Escape', async () => {
    const user = userEvent.setup()
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'a',
          name: 'Alpha.txt',
          path: 'C:\\root\\Alpha.txt',
          isDir: false,
          sizeBytes: 12,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
        {
          id: 'b',
          name: 'Beta.zip',
          path: 'C:\\root\\Beta.zip',
          isDir: false,
          sizeBytes: 24,
          itemCount: null,
          typeLabel: 'ZIP archive',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('2 items selected')).toBeInTheDocument()
    expect(screen.getByText('C:\\root\\Alpha.txt')).toBeInTheDocument()
    expect(screen.getByText('C:\\root\\Beta.zip')).toBeInTheDocument()

    await user.keyboard('{Escape}')
    expect(usePropertiesDialogStore.getState().dialog).toBeNull()
  })

  it('formats modified and created dates with the configured absolute format', () => {
    useConfigStore.setState({
      dateFormat: 'dmy',
      showTime: true,
      showSeconds: true,
      relativeDates: false,
    })
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'report',
          name: 'Report.txt',
          path: 'C:\\root\\Report.txt',
          isDir: false,
          sizeBytes: 2048,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: '2026-06-20T14:05:09Z',
          createdAt: '2026-06-01T09:00:00Z',
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('20/06/2026 14:05:09')).toBeInTheDocument()
    expect(screen.getByText('01/06/2026 09:00:00')).toBeInTheDocument()
  })

  it('keeps properties timestamps absolute even when relative dates are enabled', () => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date('2026-06-30T12:00:00Z'))
    useConfigStore.setState({
      dateFormat: 'ymd',
      showTime: true,
      showSeconds: false,
      relativeDates: true,
    })
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'fresh',
          name: 'Fresh.txt',
          path: 'C:\\root\\Fresh.txt',
          isDir: false,
          sizeBytes: 10,
          itemCount: null,
          typeLabel: 'TXT file',
          modifiedAt: '2026-06-30T11:45:00Z',
          createdAt: '2026-06-20T11:45:00Z',
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('2026-06-30 11:45')).toBeInTheDocument()
    expect(screen.getByText('2026-06-20 11:45')).toBeInTheDocument()
    expect(screen.queryByText('15 minutes ago')).not.toBeInTheDocument()
  })

  it('renders folder fallback details when native properties are unavailable', () => {
    usePropertiesDialogStore.getState().open({
      items: [
        {
          id: 'folder',
          name: 'Projects',
          path: 'C:\\root\\Projects',
          isDir: true,
          sizeBytes: null,
          itemCount: 5,
          typeLabel: 'Folder',
          modifiedAt: null,
          createdAt: null,
          attributes: [],
          isHidden: false,
          isSystem: false,
        },
      ],
    })

    render(<PropertiesDialog />)

    expect(screen.getByText('Projects')).toBeInTheDocument()
    expect(screen.getAllByText('5 items')).toHaveLength(2)
    expect(screen.getByText('Folder')).toBeInTheDocument()
  })
})

const macFile = {
  id: 'report',
  name: 'Report.pdf',
  path: '/Users/example/Report.pdf',
  isDir: false,
  sizeBytes: 100,
  itemCount: null,
  typeLabel: 'PDF document',
  modifiedAt: null,
  createdAt: null,
  attributes: [],
  isHidden: false,
  isSystem: false,
}

describe('PropertiesDialog set-default-application button', () => {
  it('shows the button on macOS for a single file with an extension', async () => {
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    expect(screen.getByRole('button', { name: 'Set Default Application…' })).toBeInTheDocument()
    await screen.findByText('Fixture Preview')
  })

  it('hides the button on Windows', () => {
    setPlatform('Win32')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    expect(
      screen.queryByRole('button', { name: 'Set Default Application…' }),
    ).not.toBeInTheDocument()
  })

  it('hides the button for a folder selection', () => {
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [{ ...macFile, isDir: true }] })

    render(<PropertiesDialog />)

    expect(
      screen.queryByRole('button', { name: 'Set Default Application…' }),
    ).not.toBeInTheDocument()
  })

  it('hides the button for a multi-selection', () => {
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({
      items: [
        macFile,
        { ...macFile, id: 'other', name: 'Other.pdf', path: '/Users/example/Other.pdf' },
      ],
    })

    render(<PropertiesDialog />)

    expect(
      screen.queryByRole('button', { name: 'Set Default Application…' }),
    ).not.toBeInTheDocument()
  })

  it('opens the default-app dialog store with the selected file on click', async () => {
    const user = userEvent.setup()
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    await user.click(screen.getByRole('button', { name: 'Set Default Application…' }))

    await waitFor(() => {
      expect(useDefaultAppDialogStore.getState().dialog).toEqual({
        filePath: '/Users/example/Report.pdf',
        fileName: 'Report.pdf',
        apps: [
          {
            name: 'Fixture Preview',
            bundlePath: '/Applications/Fixture Preview.app',
            bundleId: 'com.example.fixture-preview',
            iconDataUrl: 'data:image/png;base64,RkFLRQ==',
          },
          {
            name: 'Fixture Text Edit',
            bundlePath: '/Applications/Fixture Text Edit.app',
            bundleId: 'com.example.fixture-textedit',
            iconDataUrl: null,
          },
        ],
      })
    })
  })

  it('disables the button and shows a spinner while apps are being fetched, and does not open the picker until they resolve', async () => {
    const user = userEvent.setup()
    const deferred = createDeferred<ListApplicationsResponse>()
    ipc.override('list_applications', () => deferred.promise as never)
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    const button = screen.getByRole('button', { name: 'Set Default Application…' })
    await user.click(button)

    const loadingButton = await screen.findByRole('button', { name: 'Loading…' })
    expect(loadingButton).toBeDisabled()
    expect(useDefaultAppDialogStore.getState().dialog).toBeNull()

    await act(async () => {
      deferred.resolve({ apps: [] })
      await deferred.promise
    })

    await waitFor(() => {
      expect(useDefaultAppDialogStore.getState().dialog).not.toBeNull()
    })
    expect(screen.getByRole('button', { name: 'Set Default Application…' })).toBeInTheDocument()
  })
})

describe('PropertiesDialog default application row', () => {
  afterEach(() => {
    ipc.reset()
  })

  it('shows the current default app once loaded, on macOS for a file with an extension', async () => {
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    expect(screen.getByText('Default App')).toBeInTheDocument()
    expect(screen.getByText('Loading…')).toBeInTheDocument()
    expect(await screen.findByText('Fixture Preview')).toBeInTheDocument()
    const icon = screen.getByText('Fixture Preview').closest('dd')?.querySelector('img')
    expect(icon).toHaveAttribute('src', 'data:image/png;base64,RkFLRQ==')
  })

  it('falls back to a generic glyph when the default app has no icon', async () => {
    ipc.override('get_default_application', {
      app: {
        name: 'Fixture Text Edit',
        bundlePath: '/Applications/Fixture Text Edit.app',
        bundleId: 'com.example.fixture-textedit',
        iconDataUrl: null,
      },
    })
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    await screen.findByText('Fixture Text Edit')
    const dd = screen.getByText('Fixture Text Edit').closest('dd')
    expect(dd?.querySelector('img')).not.toBeInTheDocument()
    expect(dd?.querySelector('svg')).toBeInTheDocument()
  })

  it('shows "Not set" when there is no registered default app', async () => {
    ipc.override('get_default_application', { app: null })
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)

    expect(await screen.findByText('Not set')).toBeInTheDocument()
  })

  it('does not render the row on Windows, for folders, or for multi-selections', () => {
    setPlatform('Win32')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    const { rerender } = render(<PropertiesDialog />)
    expect(screen.queryByText('Default App')).not.toBeInTheDocument()

    setPlatform('MacIntel')
    act(() => {
      usePropertiesDialogStore.getState().open({ items: [{ ...macFile, isDir: true }] })
    })
    rerender(<PropertiesDialog />)
    expect(screen.queryByText('Default App')).not.toBeInTheDocument()

    act(() => {
      usePropertiesDialogStore.getState().open({
        items: [
          macFile,
          { ...macFile, id: 'other', name: 'Other.pdf', path: '/Users/example/Other.pdf' },
        ],
      })
    })
    rerender(<PropertiesDialog />)
    expect(screen.queryByText('Default App')).not.toBeInTheDocument()
  })

  it('refreshes after the Set Default Application picker closes', async () => {
    const user = userEvent.setup()
    setPlatform('MacIntel')
    usePropertiesDialogStore.getState().open({ items: [macFile] })

    render(<PropertiesDialog />)
    render(<DefaultAppDialog />)

    expect(await screen.findByText('Fixture Preview')).toBeInTheDocument()

    ipc.override('get_default_application', {
      app: {
        name: 'Fixture Text Edit',
        bundlePath: '/Applications/Fixture Text Edit.app',
        bundleId: 'com.example.fixture-textedit',
        iconDataUrl: null,
      },
    })

    await user.click(screen.getByRole('button', { name: 'Set Default Application…' }))
    const option = await screen.findByRole('option', { name: 'Fixture Text Edit' })
    await user.click(option)
    await user.click(screen.getByRole('button', { name: 'Change All…' }))

    await waitFor(() => {
      expect(useDefaultAppDialogStore.getState().dialog).toBeNull()
    })
    expect(await screen.findByText('Fixture Text Edit')).toBeInTheDocument()
  })
})
