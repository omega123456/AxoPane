import { useEffect, useRef, useState, type KeyboardEvent, type ReactNode } from 'react'
import {
  AlertTriangleIcon,
  CheckCircleIcon,
  FolderPlusIcon,
  InfoIcon,
  LoaderCircleIcon,
  PauseIcon,
  XIcon,
} from '@/components/icons'
import { EntryIcon } from '@/components/icons/EntryIcon'
import { Button, FolderInput, SectionLabel } from '@/components/controls'
import { DANGER_TEXT, TEXT_SECONDARY, TEXT_TERTIARY, WARNING_TEXT } from '@/lib/class-names'
import {
  compactCount,
  folderSizeStatusLabel,
  INDEX_UNAVAILABLE_LABEL,
  isIndexUnavailable,
} from '@/lib/folder-size-status'
import {
  excludeFolders,
  includeFolder,
  pickFoldersToExclude,
  retryIndexStart,
} from '@/lib/index-exclusions-commands'
import type { FolderSizeIndexStatus, IndexExclusionOutcome } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { basenameFromPath, getParentPath } from '@/stores/panes/pane-paths'
import { AutoFolderSizeRow } from './AutoFolderSizeRow'
import { IndexStorageRows } from './IndexStorageRows'
import {
  addedPaths,
  focusAfterAdd,
  focusAfterRemove,
  messagesForOutcomes,
  removalMessages,
  type ExclusionMessage,
  type ExclusionMessageTone,
} from './search-index-messages'
import type { SectionProps } from './section-props'

const STATUS_ICON_CLASS = 'h-3.5 w-3.5 shrink-0'
const MESSAGE_ICON_CLASS = 'mt-0.5 size-4 shrink-0'
const ENTER_KEY = 'Enter'
const MESSAGE_SEPARATOR = ' '
const NO_MESSAGES: ExclusionMessage[] = []

type FocusRequest = { kind: 'row'; path: string } | { kind: 'pathField' }

type FocusMoves = {
  focusRow: (path: string | null) => void
  focusRowOrPathField: (path: string | null) => void
}

function FolderSizeStatusIcon({ state }: { state: FolderSizeIndexStatus['state'] }) {
  if (state === 'upToDate') {
    return <CheckCircleIcon className={STATUS_ICON_CLASS} />
  }
  if (state === 'paused') {
    return <PauseIcon className={STATUS_ICON_CLASS} />
  }
  return (
    <LoaderCircleIcon className={`${STATUS_ICON_CLASS} animate-spin motion-reduce:animate-none`} />
  )
}

function scannedItemsSuffix(status: FolderSizeIndexStatus) {
  if (status.state !== 'indexing') {
    return ''
  }
  if (status.itemsScanned === undefined) {
    return ''
  }
  return ` ${compactCount.format(status.itemsScanned)} items`
}

function FolderSizeStatusLine({ status }: { status: FolderSizeIndexStatus }) {
  const label = folderSizeStatusLabel(status)
  const count = scannedItemsSuffix(status)

  return (
    <span className="mt-1 flex items-center gap-1">
      <FolderSizeStatusIcon state={status.state} />
      <span aria-hidden="true">
        {label}
        {count}
      </span>
      <span role="status" className="sr-only">
        {label}
      </span>
    </span>
  )
}

function IndexUnavailableWarning() {
  return (
    <span className="mt-1 flex items-center gap-2">
      <AlertTriangleIcon className={`${STATUS_ICON_CLASS} ${WARNING_TEXT}`} />
      <span className="grid min-w-0 flex-1">
        <span className={WARNING_TEXT}>{INDEX_UNAVAILABLE_LABEL}</span>
        <span>Search and folder sizes need the index.</span>
      </span>
      <Button onClick={() => void retryIndexStart()}>Try again</Button>
      <span role="status" className="sr-only">
        {INDEX_UNAVAILABLE_LABEL}
      </span>
    </span>
  )
}

function IndexStatus({ status }: { status: FolderSizeIndexStatus }) {
  if (isIndexUnavailable(status)) {
    return <IndexUnavailableWarning />
  }
  return <FolderSizeStatusLine status={status} />
}

function MacFolderSizeDescription({ status }: { status: FolderSizeIndexStatus }) {
  return (
    <>
      Sizes folders in the background as you browse. Folders you open are sized right away.
      <IndexStatus status={status} />
    </>
  )
}

export function SearchIndexSection(props: SectionProps) {
  const status = usePanesStore((state) => state.folderSizeIndexStatus)
  return (
    <>
      <p className={`mb-2 text-caption ${TEXT_TERTIARY}`}>
        Search and folder sizes use this index. Changes apply right away.
      </p>
      {status ? (
        <AutoFolderSizeRow {...props} description={<MacFolderSizeDescription status={status} />} />
      ) : null}
      <ExcludedFolders />
      {status ? <IndexStorageRows state={status.state} /> : null}
    </>
  )
}

function useExclusionFocus() {
  const removeButtons = useRef(new Map<string, HTMLButtonElement>())
  const pathField = useRef<HTMLInputElement>(null)
  const pendingFocus = useRef<FocusRequest | null>(null)
  const [focusRevision, setFocusRevision] = useState(0)

  useEffect(() => {
    const request = pendingFocus.current
    pendingFocus.current = null
    if (!request) {
      return
    }
    const target =
      request.kind === 'row' ? removeButtons.current.get(request.path) : pathField.current
    target?.focus()
  }, [focusRevision])

  const requestFocus = (request: FocusRequest) => {
    pendingFocus.current = request
    setFocusRevision((revision) => revision + 1)
  }

  const registerRemoveButton = (path: string) => (button: HTMLButtonElement | null) => {
    if (button) {
      removeButtons.current.set(path, button)
      return
    }
    removeButtons.current.delete(path)
  }

  const focusRowOrPathField = (path: string | null) => {
    if (path) {
      requestFocus({ kind: 'row', path })
      return
    }
    requestFocus({ kind: 'pathField' })
  }

  const focusRow = (path: string | null) => {
    if (path) {
      requestFocus({ kind: 'row', path })
    }
  }

  return { pathField, registerRemoveButton, focusRow, focusRowOrPathField }
}

function useExclusionActions(focus: FocusMoves) {
  const exclusions = useConfigStore((state) => state.indexExclusions)
  const [typedPath, setTypedPath] = useState('')
  const [messages, setMessages] = useState(NO_MESSAGES)

  const exclude = async (paths: string[]): Promise<IndexExclusionOutcome[]> => {
    const outcomes = await excludeFolders(paths)
    setMessages(messagesForOutcomes(outcomes))
    focus.focusRow(focusAfterAdd(outcomes))
    return outcomes
  }

  const addTypedPath = async () => {
    const path = typedPath.trim()
    if (!path) {
      return
    }
    const outcomes = await exclude([path])
    if (addedPaths(outcomes).length > 0) {
      setTypedPath('')
    }
  }

  const browse = async () => {
    const picked = await pickFoldersToExclude()
    if (picked.length > 0) {
      await exclude(picked)
    }
  }

  const remove = async (path: string) => {
    const before = exclusions
    const removed = await includeFolder(path)
    if (!removed) {
      return
    }
    setMessages(removalMessages(path))
    focus.focusRowOrPathField(focusAfterRemove(before, path))
  }

  return { exclusions, typedPath, setTypedPath, messages, addTypedPath, browse, remove }
}

function ExcludedFolders() {
  const { pathField, registerRemoveButton, focusRow, focusRowOrPathField } = useExclusionFocus()
  const actions = useExclusionActions({ focusRow, focusRowOrPathField })

  const onPathKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === ENTER_KEY) {
      void actions.addTypedPath()
    }
  }

  return (
    <>
      <SectionLabel>Excluded folders</SectionLabel>
      <p className={`mb-2 text-caption ${TEXT_TERTIARY}`}>
        AxoPane skips these folders completely. Their files and subfolders don't appear in search,
        and their sizes aren't indexed.
      </p>
      <div className="flex items-center gap-2">
        <FolderInput
          ref={pathField}
          ariaLabel="Folder to exclude"
          placeholder="/path/to/folder"
          value={actions.typedPath}
          onChange={actions.setTypedPath}
          onKeyDown={onPathKeyDown}
          className="min-w-0 flex-1"
        />
        <Button onClick={() => void actions.browse()}>
          <FolderPlusIcon className="size-4" />
          Browse…
        </Button>
        <Button onClick={() => void actions.addTypedPath()}>Add</Button>
      </div>
      <ExclusionMessages messages={actions.messages} />
      <ExclusionList
        exclusions={actions.exclusions}
        registerRemoveButton={registerRemoveButton}
        onRemove={(path) => void actions.remove(path)}
      />
    </>
  )
}

const messageToneIcons: Record<ExclusionMessageTone, ReactNode> = {
  info: <InfoIcon aria-hidden="true" className={`${MESSAGE_ICON_CLASS} ${TEXT_SECONDARY}`} />,
  warning: (
    <AlertTriangleIcon aria-hidden="true" className={`${MESSAGE_ICON_CLASS} ${WARNING_TEXT}`} />
  ),
  danger: (
    <AlertTriangleIcon aria-hidden="true" className={`${MESSAGE_ICON_CLASS} ${DANGER_TEXT}`} />
  ),
}

const messageToneText: Record<ExclusionMessageTone, string> = {
  info: TEXT_SECONDARY,
  warning: WARNING_TEXT,
  danger: DANGER_TEXT,
}

function ExclusionMessages({ messages }: { messages: ExclusionMessage[] }) {
  const announcement = messages.map((message) => message.text).join(MESSAGE_SEPARATOR)
  return (
    <>
      {messages.length > 0 ? (
        <ul aria-hidden="true" className="mt-2 grid gap-1">
          {messages.map((message) => (
            <li
              key={message.text}
              data-tone={message.tone}
              className={`flex items-start gap-2 text-caption ${messageToneText[message.tone]}`}
            >
              {messageToneIcons[message.tone]}
              <span>{message.text}</span>
            </li>
          ))}
        </ul>
      ) : null}
      <span role="status" className="sr-only">
        {announcement}
      </span>
    </>
  )
}

function ExclusionList({
  exclusions,
  registerRemoveButton,
  onRemove,
}: {
  exclusions: string[]
  registerRemoveButton: (path: string) => (button: HTMLButtonElement | null) => void
  onRemove: (path: string) => void
}) {
  if (exclusions.length === 0) {
    return <EverythingIndexed />
  }
  return (
    <ul aria-label="Excluded folders" className="mt-3 grid gap-1">
      {exclusions.map((path) => (
        <ExclusionRow
          key={path}
          path={path}
          removeButtonRef={registerRemoveButton(path)}
          onRemove={onRemove}
        />
      ))}
    </ul>
  )
}

function ExclusionRow({
  path,
  removeButtonRef,
  onRemove,
}: {
  path: string
  removeButtonRef: (button: HTMLButtonElement | null) => void
  onRemove: (path: string) => void
}) {
  const name = basenameFromPath(path)
  return (
    <li className="flex min-h-12 items-center gap-3 rounded-card border border-light-stroke bg-light-card px-4 py-2 dark:border-dark-stroke dark:bg-dark-card">
      <EntryIcon entry={{ name, isDir: true }} />
      <span className="grid min-w-0 flex-1">
        <span className="text-body text-light-text dark:text-dark-text">{name}</span>
        <span className={`break-all text-caption ${TEXT_SECONDARY}`}>{getParentPath(path)}</span>
      </span>
      <Button
        ref={removeButtonRef}
        variant="subtle"
        size="icon"
        aria-label={`Remove exclusion for ${path}`}
        onClick={() => onRemove(path)}
      >
        <XIcon className="size-4" />
      </Button>
    </li>
  )
}

function EverythingIndexed() {
  return (
    <div className="mt-3 grid gap-0.5 rounded-card px-4 py-3 ring-1 ring-inset ring-light-divider dark:ring-dark-divider">
      <span className="text-body text-light-text dark:text-dark-text">Everything is indexed</span>
      <span className={`text-caption ${TEXT_TERTIARY}`}>
        Add a folder to skip it. Folders macOS protects stay skipped until you open them.
      </span>
    </div>
  )
}
