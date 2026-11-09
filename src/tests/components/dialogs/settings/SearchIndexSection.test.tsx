import { render, screen, waitFor, within } from '@testing-library/react'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { SettingsModal } from '@/components/dialogs/SettingsModal'
import type {
  AppConfig,
  IndexExclusionOutcome,
  IndexExclusionOutcomeKind,
} from '@/lib/types/ipc'
import { ipc } from '@/tests/ipc-mock'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'

const BUILD_CACHE = '/Users/alex/Projects/app/build-cache'
const NODE_MODULES = '/Users/alex/Projects/web/node_modules'
const DIST = '/Users/alex/Projects/web/dist'
const HOME = '/Users/alex'

function exclusionsAfterAdd(outcomes: IndexExclusionOutcome[]) {
  const kept = useConfigStore.getState().indexExclusions
  const added = outcomes
    .filter((outcome) => ['added', 'mergedChildren', 'homeWarning'].includes(outcome.outcome))
    .map((outcome) => outcome.path)
  return [...kept, ...added].sort()
}

function respondToAdd(outcomes: IndexExclusionOutcome[]) {
  const add = vi.fn(() => ({ exclusions: exclusionsAfterAdd(outcomes), outcomes }))
  ipc.override('add_index_exclusion', add)
  return add
}

async function openSearchIndex(exclusions: string[] = []) {
  useConfigStore.setState({ indexExclusions: exclusions })
  useSettingsStore.getState().open('searchIndex')
  render(<SettingsModal />)
  await screen.findByText('AxoPane 0.1.0')
}

function indexSwitch(name: 'Folder index' | 'File index') {
  return screen.getByRole('switch', { name })
}

function savedIndexToggles() {
  const { indexFolders, indexFiles } = useConfigStore.getState()
  return { indexFolders, indexFiles }
}

function showIndexAsOff() {
  usePanesStore.setState({
    folderSizeIndexStatus: {
      backend: 'macIndex',
      isAvailable: false,
      state: 'off',
      fileNamesAvailable: false,
    },
  })
}

function pathField() {
  return screen.getByRole('textbox', { name: 'Folder to exclude' })
}

function removeButton(path: string) {
  return screen.getByRole('button', { name: `Remove exclusion for ${path}` })
}

function messageLines() {
  return Array.from(document.querySelectorAll('li[data-tone]')).map((line) => ({
    tone: line.getAttribute('data-tone'),
    text: line.textContent,
  }))
}

function announcement() {
  const regions = screen.getAllByRole('status')
  return regions[regions.length - 1]
}

describe('SearchIndexSection', () => {
  beforeEach(() => {
    ipc.install()
    ipc.override('save_config', (payload) => payload.config)
    useSettingsStore.getState().close()
    useConfigStore.getState().reset()
    usePanesStore.getState().reset()
    usePanesStore.setState({
      folderSizeIndexStatus: {
        backend: 'macIndex',
        isAvailable: true,
        state: 'upToDate',
        fileNamesAvailable: true,
      },
    })
  })

  it('starts with both indexes off and shows only the two toggles', async () => {
    showIndexAsOff()
    await openSearchIndex()

    expect(indexSwitch('Folder index')).toHaveAttribute('aria-checked', 'false')
    expect(indexSwitch('File index')).toHaveAttribute('aria-checked', 'false')
    expect(
      screen.getByText(/Folder sizes and folder search need only this index\./),
    ).toBeInTheDocument()
    expect(screen.getByText(/so search finds files too\./)).toBeInTheDocument()
    expect(screen.queryByRole('status')).not.toBeInTheDocument()
    expect(screen.queryByRole('textbox', { name: 'Folder to exclude' })).not.toBeInTheDocument()
    expect(screen.queryByText('Rebuild index')).not.toBeInTheDocument()
    expect(
      screen.queryByRole('switch', { name: 'Automatically calculate folder sizes' }),
    ).not.toBeInTheDocument()
  })

  it('turns on the folder index alone and saves it', async () => {
    const user = userEvent.setup()
    const save = vi.fn((payload: { config: AppConfig }) => payload.config)
    ipc.override('save_config', save)
    showIndexAsOff()
    await openSearchIndex()

    await user.click(indexSwitch('Folder index'))

    await waitFor(() => expect(save).toHaveBeenCalledTimes(1))
    expect(savedIndexToggles()).toEqual({ indexFolders: true, indexFiles: false })
    expect(save.mock.calls[0][0].config).toMatchObject({ indexFolders: true, indexFiles: false })
    expect(indexSwitch('Folder index')).toHaveAttribute('aria-checked', 'true')
    expect(indexSwitch('File index')).toHaveAttribute('aria-checked', 'false')
  })

  it('turns on the folder index together with the file index', async () => {
    const user = userEvent.setup()
    showIndexAsOff()
    await openSearchIndex()

    await user.click(indexSwitch('File index'))

    await waitFor(() =>
      expect(savedIndexToggles()).toEqual({ indexFolders: true, indexFiles: true }),
    )
    expect(indexSwitch('Folder index')).toHaveAttribute('aria-checked', 'true')
  })

  it('turns off the file index alone and keeps the folder index', async () => {
    const user = userEvent.setup()
    useConfigStore.setState({ indexFolders: true, indexFiles: true })
    await openSearchIndex()

    await user.click(indexSwitch('File index'))

    await waitFor(() =>
      expect(savedIndexToggles()).toEqual({ indexFolders: true, indexFiles: false }),
    )
  })

  it('turns off the file index together with the folder index', async () => {
    const user = userEvent.setup()
    useConfigStore.setState({ indexFolders: true, indexFiles: true })
    await openSearchIndex()

    await user.click(indexSwitch('Folder index'))

    await waitFor(() =>
      expect(savedIndexToggles()).toEqual({ indexFolders: false, indexFiles: false }),
    )
    expect(indexSwitch('File index')).toHaveAttribute('aria-checked', 'false')
  })

  it.each([
    [false, 'Folder index'],
    [true, 'Folder index and file index'],
  ])('reports progress in its own section (file index on: %s)', async (indexFiles, scope) => {
    useConfigStore.setState({ indexFolders: true, indexFiles })
    await openSearchIndex()

    const heading = screen.getByRole('heading', { name: 'Index progress' })
    const row = heading.nextElementSibling as HTMLElement
    expect(within(row).getByRole('status')).toHaveTextContent('Up to date')
    expect(within(row).getByText(scope)).toBeInTheDocument()
    for (const title of ['Folder index', 'File index']) {
      const toggleRow = screen.getByRole('switch', { name: title }).closest('.rounded-card')
      expect(within(toggleRow as HTMLElement).queryByRole('status')).not.toBeInTheDocument()
    }
    expect(
      screen.getByRole('switch', { name: 'Automatically calculate folder sizes' }),
    ).toBeInTheDocument()
  })

  it('shows the empty state until a folder is excluded', async () => {
    await openSearchIndex()

    expect(screen.getByText('Everything is indexed')).toBeInTheDocument()
    expect(
      screen.getByText(
        'Add a folder to skip it. Folders macOS protects stay skipped until you open them.',
      ),
    ).toBeInTheDocument()
    expect(pathField()).toHaveAttribute('placeholder', '/path/to/folder')
  })

  it('lists each exclusion with its name, parent path and a remove button', async () => {
    await openSearchIndex([BUILD_CACHE, NODE_MODULES])

    const list = screen.getByRole('list', { name: 'Excluded folders' })
    expect(within(list).getAllByRole('listitem')).toHaveLength(2)
    expect(within(list).getByText('build-cache')).toBeInTheDocument()
    expect(within(list).getByText('/Users/alex/Projects/app')).toBeInTheDocument()
    expect(removeButton(NODE_MODULES)).toHaveClass('size-8')
    expect(screen.queryByText('Everything is indexed')).not.toBeInTheDocument()
  })

  it('adds a typed path on Enter, clears the field and focuses the new row', async () => {
    const user = userEvent.setup()
    const add = respondToAdd([{ path: BUILD_CACHE, outcome: 'added' }])
    await openSearchIndex()

    await user.type(pathField(), `  ${BUILD_CACHE}  {Enter}`)

    await waitFor(() => expect(removeButton(BUILD_CACHE)).toHaveFocus())
    expect(add).toHaveBeenCalledWith({ paths: [BUILD_CACHE] })
    expect(pathField()).toHaveValue('')
    expect(messageLines()).toEqual([{ tone: 'info', text: 'Added build-cache.' }])
    expect(announcement()).toHaveTextContent('Added build-cache.')
    expect(useConfigStore.getState().indexExclusions).toEqual([BUILD_CACHE])
  })

  it('adds a typed path with the Add button', async () => {
    const user = userEvent.setup()
    const add = respondToAdd([{ path: DIST, outcome: 'added' }])
    await openSearchIndex()

    await user.type(pathField(), DIST)
    await user.click(screen.getByRole('button', { name: 'Add' }))

    await waitFor(() => expect(removeButton(DIST)).toHaveFocus())
    expect(add).toHaveBeenCalledWith({ paths: [DIST] })
  })

  it('ignores Add when the field is blank', async () => {
    const user = userEvent.setup()
    const add = respondToAdd([])
    await openSearchIndex()

    await user.click(screen.getByRole('button', { name: 'Add' }))
    await user.type(pathField(), '   {Enter}')

    expect(add).not.toHaveBeenCalled()
  })

  it.each([
    [
      'insideExisting',
      { path: `${NODE_MODULES}/react`, outcome: 'insideExisting', detail: NODE_MODULES },
      'info',
      'Already inside node_modules, which is excluded. Remove that exclusion to index this folder.',
    ],
    [
      'alwaysSkipped',
      { path: '/Volumes', outcome: 'alwaysSkipped' },
      'info',
      'AxoPane always skips /Volumes.',
    ],
    [
      'rootRejected',
      { path: '/', outcome: 'rootRejected' },
      'danger',
      "You can't exclude the whole disk.",
    ],
    [
      'notFound',
      { path: '/Users/alex/missing', outcome: 'notFound' },
      'danger',
      "/Users/alex/missing doesn't exist.",
    ],
    [
      'notAFolder',
      { path: '/Users/alex/notes.txt', outcome: 'notAFolder' },
      'danger',
      '/Users/alex/notes.txt is not a folder.',
    ],
  ] as const)(
    'reports %s without adding anything and keeps focus in the field',
    async (_kind, outcome, tone, text) => {
      const user = userEvent.setup()
      respondToAdd([outcome as IndexExclusionOutcome])
      await openSearchIndex([NODE_MODULES])

      await user.type(pathField(), `${outcome.path}{Enter}`)

      await waitFor(() => expect(messageLines()).toEqual([{ tone, text }]))
      expect(pathField()).toHaveFocus()
      expect(pathField()).toHaveValue(outcome.path)
      expect(announcement()).toHaveTextContent(text)
    },
  )

  it('moves focus to the existing row for a duplicate', async () => {
    const user = userEvent.setup()
    respondToAdd([{ path: BUILD_CACHE, outcome: 'duplicate', detail: BUILD_CACHE }])
    await openSearchIndex([BUILD_CACHE, NODE_MODULES])

    await user.type(pathField(), `${BUILD_CACHE}{Enter}`)

    await waitFor(() => expect(removeButton(BUILD_CACHE)).toHaveFocus())
    expect(messageLines()).toEqual([{ tone: 'info', text: 'build-cache is already excluded.' }])
  })

  it.each([
    [
      'mergedChildren' as IndexExclusionOutcomeKind,
      { path: '/Users/alex/Projects', outcome: 'mergedChildren', mergedCount: 2 },
      [
        { tone: 'info', text: 'Added Projects.' },
        {
          tone: 'info',
          text: 'Projects already contained 2 excluded folders, so the list now shows only Projects.',
        },
      ],
    ],
    [
      'homeWarning' as IndexExclusionOutcomeKind,
      { path: HOME, outcome: 'homeWarning' },
      [
        { tone: 'info', text: 'Added alex.' },
        {
          tone: 'warning',
          text: "Your home folder is excluded, so most of your files won't appear in search.",
        },
      ],
    ],
  ] as const)('adds and explains %s', async (_kind, outcome, lines) => {
    const user = userEvent.setup()
    respondToAdd([outcome as IndexExclusionOutcome])
    await openSearchIndex()

    await user.type(pathField(), `${outcome.path}{Enter}`)

    await waitFor(() => expect(removeButton(outcome.path)).toHaveFocus())
    expect(messageLines()).toEqual(lines)
  })

  it('adds picked folders and summarises multiple outcomes in picker order', async () => {
    const user = userEvent.setup()
    ipc.override('pick_index_exclusions', { paths: [DIST, NODE_MODULES, '/Volumes'] })
    const add = respondToAdd([
      { path: DIST, outcome: 'added' },
      { path: NODE_MODULES, outcome: 'added' },
      { path: '/Volumes', outcome: 'alwaysSkipped' },
    ])
    await openSearchIndex()

    await user.click(screen.getByRole('button', { name: 'Browse…' }))

    await waitFor(() => expect(removeButton(NODE_MODULES)).toHaveFocus())
    expect(add).toHaveBeenCalledWith({ paths: [DIST, NODE_MODULES, '/Volumes'] })
    expect(messageLines()).toEqual([
      { tone: 'info', text: 'Added 2 folders.' },
      { tone: 'info', text: 'AxoPane always skips /Volumes.' },
    ])
  })

  it('does nothing when the picker is cancelled', async () => {
    const user = userEvent.setup()
    const add = respondToAdd([])
    await openSearchIndex()

    const browse = screen.getByRole('button', { name: 'Browse…' })
    await user.click(browse)

    await waitFor(() => expect(browse).toHaveFocus())
    expect(add).not.toHaveBeenCalled()
    expect(messageLines()).toEqual([])
  })

  it.each([
    [BUILD_CACHE, DIST],
    [NODE_MODULES, DIST],
  ])('removes %s immediately and focuses the neighbouring row', async (removed, focused) => {
    const user = userEvent.setup()
    const exclusions = [BUILD_CACHE, DIST, NODE_MODULES]
    const remove = vi.fn(({ path }: { path: string }) => ({
      exclusions: exclusions.filter((excluded) => excluded !== path),
    }))
    ipc.override('remove_index_exclusion', remove)
    await openSearchIndex(exclusions)

    await user.click(removeButton(removed))

    await waitFor(() => expect(removeButton(focused)).toHaveFocus())
    expect(remove).toHaveBeenCalledWith({ path: removed })
    expect(screen.queryByRole('button', { name: `Remove exclusion for ${removed}` })).toBeNull()
    expect(messageLines()).toEqual([
      {
        tone: 'info',
        text: `Removed ${removed.split('/').at(-1)}. Updating the index in the background.`,
      },
    ])
  })

  it('returns focus to the path field after removing the last exclusion', async () => {
    const user = userEvent.setup()
    ipc.override('remove_index_exclusion', { exclusions: [] })
    await openSearchIndex([BUILD_CACHE])

    await user.click(removeButton(BUILD_CACHE))

    await waitFor(() => expect(pathField()).toHaveFocus())
    expect(screen.getByText('Everything is indexed')).toBeInTheDocument()
  })

  it('keeps the list and shows nothing when the commands fail', async () => {
    const user = userEvent.setup()
    ipc.override('remove_index_exclusion', () => Promise.reject(new Error('unsupported')))
    ipc.override('add_index_exclusion', () => Promise.reject(new Error('unsupported')))
    ipc.override('pick_index_exclusions', () => Promise.reject(new Error('unsupported')))
    await openSearchIndex([BUILD_CACHE])

    await user.click(removeButton(BUILD_CACHE))
    await user.type(pathField(), `${DIST}{Enter}`)
    await user.click(screen.getByRole('button', { name: 'Browse…' }))

    await waitFor(() => expect(screen.getByRole('button', { name: 'Browse…' })).toHaveFocus())
    expect(removeButton(BUILD_CACHE)).toBeInTheDocument()
    expect(messageLines()).toEqual([])
    expect(useConfigStore.getState().indexExclusions).toEqual([BUILD_CACHE])
  })

  it('keeps the warning row in place when Try again fails', async () => {
    const user = userEvent.setup()
    ipc.override('retry_folder_size_index', () => Promise.reject(new Error('unsupported')))
    usePanesStore.setState({
      folderSizeIndexStatus: {
        backend: 'macIndex',
        isAvailable: false,
        state: 'unavailable',
        fileNamesAvailable: false,
      },
    })
    await openSearchIndex()

    await user.click(screen.getByRole('button', { name: 'Try again' }))

    expect(screen.getByRole('button', { name: 'Try again' })).toBeInTheDocument()
  })
})
