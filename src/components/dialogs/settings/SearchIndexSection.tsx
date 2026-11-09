import {
  AlertTriangleIcon,
  CheckCircleIcon,
  FileIcon,
  FolderIcon,
  LoaderCircleIcon,
  PauseIcon,
} from '@/components/icons'
import { Button, SectionLabel, SettingRow, ToggleSwitch } from '@/components/controls'
import { TEXT_TERTIARY, WARNING_TEXT } from '@/lib/class-names'
import {
  compactCount,
  folderSizeStatusLabel,
  INDEX_UNAVAILABLE_LABEL,
  isIndexRunning,
  isIndexUnavailable,
} from '@/lib/folder-size-status'
import { retryIndexStart } from '@/lib/index-exclusions-commands'
import type { FolderSizeIndexStatus } from '@/lib/types/ipc'
import { usePanesStore } from '@/stores/panes-store'
import { AutoFolderSizeRow } from './AutoFolderSizeRow'
import { ExcludedFolders } from './ExcludedFolders'
import { IndexStorageRows } from './IndexStorageRows'
import { SETTINGS_ICON_CLASS as iconClass, type SectionProps } from './section-props'
import { withFileIndex, withFolderIndex } from './settings-draft'

const FOLDER_INDEX_TITLE = 'Folder index'
const FILE_INDEX_TITLE = 'File index'
const INDEX_PROGRESS_TITLE = 'Index progress'
const FOLDERS_ONLY_SCOPE = 'Folder index'
const FOLDERS_AND_FILES_SCOPE = 'Folder index and file index'
const AUTO_FOLDER_SIZE_COPY =
  'Sizes folders in the background as you browse, using the folder index. Folders you open are sized right away.'

function ProgressIcon({ state }: { state: FolderSizeIndexStatus['state'] }) {
  if (state === 'upToDate') {
    return <CheckCircleIcon className={iconClass} />
  }
  if (state === 'paused') {
    return <PauseIcon className={iconClass} />
  }
  return <LoaderCircleIcon className={`${iconClass} animate-spin motion-reduce:animate-none`} />
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

function ProgressTitle({ status }: { status: FolderSizeIndexStatus }) {
  const label = folderSizeStatusLabel(status)
  return (
    <>
      <span aria-hidden="true">
        {label}
        {scannedItemsSuffix(status)}
      </span>
      <span role="status" className="sr-only">
        {label}
      </span>
    </>
  )
}

function UnavailableTitle() {
  return (
    <>
      <span className={WARNING_TEXT}>{INDEX_UNAVAILABLE_LABEL}</span>
      <span role="status" className="sr-only">
        {INDEX_UNAVAILABLE_LABEL}
      </span>
    </>
  )
}

function indexedScopeLabel(indexFiles: boolean) {
  return indexFiles ? FOLDERS_AND_FILES_SCOPE : FOLDERS_ONLY_SCOPE
}

type IndexProgressProps = { status: FolderSizeIndexStatus; indexFiles: boolean }

function IndexProgressRow({ status, indexFiles }: IndexProgressProps) {
  if (isIndexUnavailable(status)) {
    return (
      <SettingRow
        icon={<AlertTriangleIcon className={`${iconClass} ${WARNING_TEXT}`} />}
        title={<UnavailableTitle />}
        description="Search and folder sizes need the index."
        control={<Button onClick={() => void retryIndexStart()}>Try again</Button>}
      />
    )
  }
  return (
    <SettingRow
      icon={<ProgressIcon state={status.state} />}
      title={<ProgressTitle status={status} />}
      description={indexedScopeLabel(indexFiles)}
      control={null}
    />
  )
}

function IndexProgress(props: IndexProgressProps) {
  return (
    <>
      <SectionLabel>{INDEX_PROGRESS_TITLE}</SectionLabel>
      <IndexProgressRow {...props} />
    </>
  )
}

type IndexToggleProps = Pick<SectionProps, 'draft' | 'updateDraft'>

function isRunning(status: FolderSizeIndexStatus | null): status is FolderSizeIndexStatus {
  return status !== null && isIndexRunning(status)
}

function FolderIndexRow({ draft, updateDraft }: IndexToggleProps) {
  return (
    <SettingRow
      icon={<FolderIcon className={iconClass} />}
      title={FOLDER_INDEX_TITLE}
      description="Indexes folder names and sizes. Folder sizes and folder search need only this index."
      control={
        <ToggleSwitch
          label={FOLDER_INDEX_TITLE}
          checked={draft.indexFolders}
          onChange={(enabled) => updateDraft((current) => withFolderIndex(current, enabled))}
        />
      }
    />
  )
}

function FileIndexRow({ draft, updateDraft }: IndexToggleProps) {
  return (
    <SettingRow
      icon={<FileIcon className={iconClass} />}
      title={FILE_INDEX_TITLE}
      description="Adds file names to the folder index so search finds files too. Turning it on also turns on the folder index."
      control={
        <ToggleSwitch
          label={FILE_INDEX_TITLE}
          checked={draft.indexFiles}
          onChange={(enabled) => updateDraft((current) => withFileIndex(current, enabled))}
        />
      }
    />
  )
}

function RunningIndexSettings({
  status,
  ...props
}: SectionProps & { status: FolderSizeIndexStatus }) {
  return (
    <>
      <AutoFolderSizeRow {...props} description={AUTO_FOLDER_SIZE_COPY} />
      <IndexProgress status={status} indexFiles={props.draft.indexFiles} />
      <ExcludedFolders />
      <IndexStorageRows state={status.state} />
    </>
  )
}

export function SearchIndexSection(props: SectionProps) {
  const status = usePanesStore((state) => state.folderSizeIndexStatus)
  return (
    <>
      <p className={`mb-2 text-caption ${TEXT_TERTIARY}`}>
        Search and folder sizes use these indexes. Both are off until you turn them on. Changes
        apply right away.
      </p>
      <FolderIndexRow draft={props.draft} updateDraft={props.updateDraft} />
      <FileIndexRow draft={props.draft} updateDraft={props.updateDraft} />
      {isRunning(status) ? <RunningIndexSettings {...props} status={status} /> : null}
    </>
  )
}
