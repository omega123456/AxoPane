import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SearchPalette } from '@/components/search/SearchPalette'
import { ipc } from '@/tests/ipc-mock'
import type { FolderSizeIndexStatus, SearchHit, SearchPage } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useRecentFoldersStore } from '@/stores/recent-folders-store'
import { useSearchPaletteStore } from '@/stores/search-palette-store'

type PanesState = ReturnType<typeof usePanesStore.getState>

const original = usePanesStore.getState()
let navigatePane: ReturnType<typeof vi.fn<PanesState['navigatePane']>>
let openTabFromPath: ReturnType<typeof vi.fn<PanesState['openTabFromPath']>>

function hit(name: string, isDir = false, parent = 'C:\\Work'): SearchHit {
  return {
    path: `${parent}\\${name}`,
    name,
    parentPath: parent,
    isDir,
    modifiedAt: null,
    nameRanges: [[0, 3]],
    parentRanges: [],
  }
}

function page(
  items: SearchHit[],
  total = items.length,
  extra: Partial<SearchPage> = {},
): SearchPage {
  return {
    queryId: 1,
    offset: 0,
    total,
    truncated: false,
    items,
    capabilities: { backend: 'everything', foldersOnly: false },
    ...extra,
  }
}

const everythingStatus: FolderSizeIndexStatus = {
  backend: 'everything',
  isAvailable: true,
  state: 'upToDate',
}

function setIndexStatus(status: FolderSizeIndexStatus) {
  ipc.override('folder_size_index_status', status)
  act(() => usePanesStore.setState({ folderSizeIndexStatus: status }))
}

async function renderPalette() {
  const view = render(
    <>
      <SearchPalette />
      <section data-pane-id="left" tabIndex={-1} aria-label="Left pane" />
      <button type="button" data-search-trigger="">
        Trigger
      </button>
      <button type="button">Elsewhere</button>
    </>,
  )
  await openPalette()
  return view
}

/** Opens the palette and lets its index-status refresh settle. */
async function openPalette() {
  await act(async () => useSearchPaletteStore.getState().open())
}

const input = () => screen.getByRole('combobox', { name: 'Search everywhere' })
const options = () => screen.queryAllByRole('option')

async function search(user: ReturnType<typeof userEvent.setup>, query: string) {
  await user.clear(input())
  await user.type(input(), query)
  await waitFor(() => expect(useSearchPaletteStore.getState().status).not.toBe('loading'))
}

beforeEach(() => {
  navigatePane = vi.fn<PanesState['navigatePane']>(() => Promise.resolve())
  openTabFromPath = vi.fn<PanesState['openTabFromPath']>(() => Promise.resolve())
  usePanesStore.getState().reset()
  usePanesStore.setState({ navigatePane, openTabFromPath, activePaneId: 'left' })
  useConfigStore.getState().reset()
  useRecentFoldersStore.getState().hydrate([])
  useSearchPaletteStore.getState().reset()
  setIndexStatus(everythingStatus)
})

afterEach(() => {
  act(() => {
    useSearchPaletteStore.getState().reset()
    usePanesStore.setState({
      navigatePane: original.navigatePane,
      openTabFromPath: original.openTabFromPath,
    })
  })
})

describe('SearchPalette', () => {
  it('renders nothing while closed', () => {
    render(<SearchPalette />)
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('offers favourites and recent folders before a query', async () => {
    const user = userEvent.setup()
    act(() => {
      useConfigStore.setState({
        favourites: ['C:\\Users\\alex\\Desktop'],
        favouriteNames: { 'C:\\Users\\alex\\Desktop': 'My desk' },
      })
      useRecentFoldersStore
        .getState()
        .hydrate(['C:\\Users\\alex\\Desktop', 'D:\\Photos\\2026', 'C:\\dev'])
    })
    await renderPalette()
    expect(input()).toHaveFocus()
    const listbox = screen.getByRole('listbox', { name: 'Favourites and recent folders' })
    expect(
      within(listbox)
        .getAllByRole('option')
        .map((o) => o.textContent),
    ).toEqual(['My deskC:\\Users\\alex', '2026D:\\Photos', 'devC:\\'])
    await user.keyboard('{ArrowDown}{ArrowDown}{ArrowDown}{ArrowUp}{PageDown}{PageUp}')
    expect(input()).toHaveAttribute('aria-activedescendant', 'search-option-0')
    await user.keyboard('{Control>}{End}{/Control}')
    expect(input()).toHaveAttribute('aria-activedescendant', 'search-option-2')
    await user.keyboard('{Control>}{Home}{/Control}{ArrowDown}{Tab}')
    expect(input()).toHaveValue('D:\\Photos\\2026\\')
    expect(screen.getByText('Everything')).toBeInTheDocument()
  })

  it('opens a recent folder with Enter', async () => {
    const user = userEvent.setup()
    act(() => useRecentFoldersStore.getState().hydrate(['/Users/alex/Projects']))
    await renderPalette()
    await user.keyboard('{Tab}')
    expect(input()).toHaveValue('/Users/alex/Projects/')
    await user.clear(input())
    await user.keyboard('{Enter}')
    expect(openTabFromPath).toHaveBeenCalledWith('left', '/Users/alex/Projects', {
      revealPath: undefined,
    })
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(screen.getByRole('region', { name: 'Left pane' })).toHaveFocus()
  })

  it('shows ranked results, highlights and counts', async () => {
    const user = userEvent.setup()
    await renderPalette()
    await user.type(input(), 'in')
    expect(screen.getByText('Keep typing. Search starts at 3 characters')).toBeInTheDocument()
    await search(user, 'inv')
    expect(options().map((o) => o.getAttribute('aria-setsize'))).toEqual(['2', '2'])
    expect(screen.getByText('Everything · 2 results')).toBeInTheDocument()
    expect(screen.getByRole('status', { name: '' }) ?? null).toBeTruthy()
    expect(screen.getByText('2 results')).toBeInTheDocument()
    expect(options()[0].querySelector('mark')?.textContent).toBe('Invoice')
  })

  it('activates hits with Enter variants and completes folders with Tab', async () => {
    const user = userEvent.setup()
    const opened: string[] = []
    ipc.override('open_path', (request) => {
      opened.push(request.path)
    })
    await renderPalette()
    await search(user, 'inv')
    await user.keyboard('{Tab}')
    expect(input()).toHaveValue('Invoices/')

    await search(user, 'inv')
    await user.keyboard('{ArrowDown}')
    await user.keyboard('{Shift>}{Enter}{/Shift}')
    expect(opened).toEqual(['C:\\Users\\alex\\Documents\\invoice-0412.pdf'])

    await openPalette()
    await search(user, 'inv')
    await user.keyboard('{ArrowDown}{Control>}{Enter}{/Control}')
    expect(openTabFromPath).toHaveBeenCalledWith('left', 'C:\\Users\\alex\\Documents', {
      revealPath: 'C:\\Users\\alex\\Documents\\invoice-0412.pdf',
    })

    await openPalette()
    await search(user, 'inv')
    await user.keyboard('{Enter}')
    expect(openTabFromPath).toHaveBeenLastCalledWith('left', 'C:\\Users\\alex\\Invoices', {
      revealPath: undefined,
    })
  })

  it('clears the query on Escape, then closes', async () => {
    const user = userEvent.setup()
    await renderPalette()
    await search(user, 'inv')
    await user.keyboard('{Escape}')
    expect(input()).toHaveValue('')
    await user.keyboard('{Escape}')
    expect(screen.queryByRole('dialog')).toBeNull()
  })

  it('keeps the palette open and reselects the query on its own shortcut', async () => {
    const user = userEvent.setup()
    await renderPalette()
    await user.type(input(), 'inv')
    await user.keyboard('{F4}')
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    expect((input() as HTMLInputElement).selectionStart).toBe(0)
    expect((input() as HTMLInputElement).selectionEnd).toBe(3)
  })

  it('activates rows with the mouse and follows the hover', async () => {
    const user = userEvent.setup()
    const opened: string[] = []
    ipc.override('open_path', (request) => {
      opened.push(request.path)
    })
    await renderPalette()
    await search(user, 'inv')
    fireEvent.mouseMove(options()[1])
    expect(input()).toHaveAttribute('aria-activedescendant', 'search-option-1')
    await user.keyboard('{Shift>}')
    await user.click(options()[1])
    await user.keyboard('{/Shift}')
    expect(opened).toHaveLength(1)

    await openPalette()
    await search(user, 'inv')
    await user.keyboard('{Control>}')
    await user.click(options()[0])
    await user.keyboard('{/Control}')
    expect(openTabFromPath).toHaveBeenCalledTimes(1)

    await openPalette()
    await search(user, 'inv')
    await user.click(options()[0])
    expect(openTabFromPath).toHaveBeenCalledTimes(2)
    expect(navigatePane).not.toHaveBeenCalled()
  })

  it('reruns with a new scope and disables files for a folder-only index', async () => {
    const user = userEvent.setup()
    const scopes: string[] = []
    ipc.override('search_query', (request) => {
      scopes.push(request.scope)
      return page([hit('Invoices', true)])
    })
    await renderPalette()
    await search(user, 'inv')
    await user.click(screen.getByRole('button', { name: 'Folders' }))
    await waitFor(() => expect(scopes).toEqual(['all', 'folders']))
    expect(screen.getByRole('button', { name: 'Folders' })).toHaveAttribute('aria-pressed', 'true')
    expect(input()).toHaveFocus()

    act(() => useSearchPaletteStore.getState().reset())
    setIndexStatus({ backend: 'macIndex', isAvailable: true, state: 'upToDate' })
    await openPalette()
    expect(screen.getByRole('button', { name: 'Files' })).toBeDisabled()
    expect(screen.getByText('Folder names only on this Mac')).toBeInTheDocument()
    expect(input()).toHaveAttribute('placeholder', 'Search folders, or type a path like alex/')
    expect(screen.getByText('Folders on this Mac')).toBeInTheDocument()
  })

  it('explains an unavailable backend', async () => {
    const user = userEvent.setup()
    ipc.override('search_query', () => Promise.reject({ kind: 'notReady' }))
    await renderPalette()
    await search(user, 'inv')
    expect(screen.getByText('Everything isn’t running')).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'Get Everything' })).toHaveAttribute(
      'href',
      'https://www.voidtools.com/downloads/',
    )
    expect(input()).toHaveAttribute('aria-expanded', 'false')

    ipc.override('search_query', () =>
      Promise.reject({ kind: 'failed', message: 'IPC queue full' }),
    )
    await search(user, 'invo')
    expect(screen.getByText('Search failed')).toBeInTheDocument()
    expect(screen.getByText('IPC queue full')).toBeInTheDocument()
  })

  it('explains the macOS index states', async () => {
    const user = userEvent.setup()
    setIndexStatus({ backend: 'macIndex', isAvailable: false, state: 'notReady' })
    ipc.override('search_query', () => Promise.reject({ kind: 'notReady' }))
    await renderPalette()
    await search(user, 'proj')
    expect(screen.getByText('The folder index is starting')).toBeInTheDocument()
    ipc.override('search_query', () => Promise.reject({ kind: 'unavailable' }))
    await search(user, 'proje')
    expect(screen.getByText('Search is unavailable')).toBeInTheDocument()
  })

  it('says when nothing matched', async () => {
    const user = userEvent.setup()
    ipc.override('search_query', page([]))
    await renderPalette()
    await search(user, 'taxes 2019')
    expect(screen.getByText('No results for “taxes 2019”')).toBeInTheDocument()
    expect(screen.getByText('No results')).toBeInTheDocument()

    ipc.override(
      'search_query',
      page([], 0, { capabilities: { backend: 'macIndex', foldersOnly: true } }),
    )
    await search(user, 'tax return')
    expect(screen.getByText('No folders match “tax return”')).toBeInTheDocument()
  })

  it('reports macOS indexing progress and pauses in the footer', async () => {
    const user = userEvent.setup()
    setIndexStatus({
      backend: 'macIndex',
      isAvailable: true,
      state: 'indexing',
      itemsScanned: 1_204_388,
    })
    ipc.override(
      'search_query',
      page([hit('Projects', true, '/Users/alex')], 1, {
        capabilities: { backend: 'macIndex', foldersOnly: true },
      }),
    )
    await renderPalette()
    expect(
      screen.getByText('Indexing… 1.2M folders · results may be incomplete'),
    ).toBeInTheDocument()
    setIndexStatus({ backend: 'macIndex', isAvailable: true, state: 'indexing' })
    expect(screen.getByText('Indexing… · results may be incomplete')).toBeInTheDocument()
    setIndexStatus({
      backend: 'macIndex',
      isAvailable: true,
      state: 'paused',
      pauseReason: 'thermal',
    })
    expect(screen.getByText('Paused to keep your Mac cool')).toBeInTheDocument()
    setIndexStatus({ backend: 'macIndex', isAvailable: true, state: 'upToDate' })
    await search(user, 'proj')
    expect(screen.getByText('Folders on this Mac · 1 result')).toBeInTheDocument()
  })

  it('loads more results at the end of the list', async () => {
    const user = userEvent.setup()
    const first = Array.from({ length: 5 }, (_, index) => hit(`report-${index}.docx`))
    ipc.override('search_query', page(first, 6, { truncated: true }))
    ipc.override('search_page', () => page([hit('report-5.docx')], 6, { offset: 5 }))
    await renderPalette()
    await search(user, 'report')
    await waitFor(() => expect(options()).toHaveLength(6))
    expect(screen.getByText('Everything · 6 results · refine your search')).toBeInTheDocument()

    const many = Array.from({ length: 50 }, (_, index) => hit(`item-${index}.txt`))
    let pageRequests = 0
    let release: () => void = () => {}
    ipc.override('search_query', page(many, 60))
    ipc.override('search_page', (request) => {
      pageRequests += 1
      return new Promise<SearchPage>((resolve) => {
        release = () =>
          resolve(page([hit(`item-${request.offset}.txt`)], 60, { offset: request.offset }))
      })
    })
    await search(user, 'item')
    expect(screen.getByText('Everything · Showing 50 of 60')).toBeInTheDocument()
    await user.keyboard('{Control>}{End}{/Control}')
    await waitFor(() => expect(useSearchPaletteStore.getState().loadingMore).toBe(true))
    await act(async () => release())
    await waitFor(() => expect(useSearchPaletteStore.getState().items).toHaveLength(51))
    expect(pageRequests).toBe(1)
  })

  it('closes on a click outside but not on its trigger', async () => {
    const user = userEvent.setup()
    await renderPalette()
    await user.click(screen.getByRole('button', { name: 'Trigger' }))
    expect(screen.getByRole('dialog')).toBeInTheDocument()
    await user.click(screen.getByRole('button', { name: 'Elsewhere' }))
    expect(screen.queryByRole('dialog')).toBeNull()
  })
})
