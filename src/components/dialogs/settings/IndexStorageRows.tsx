import { useEffect, useState } from 'react'
import { FolderOpenIcon, HardDriveIcon, RotateCcwIcon } from '@/components/icons'
import { Button, SectionLabel, SettingRow } from '@/components/controls'
import { countLabel, DETAIL_SEPARATOR, formatBytes } from '@/lib/format'
import { loadIndexStorage, rebuildIndex } from '@/lib/index-storage-commands'
import type { FolderSizeIndexStatus, FolderSizeIndexStorage } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { openTabInActivePane } from '@/stores/panes-store'
import { useSettingsStore } from '@/stores/settings-store'
import { SETTINGS_ICON_CLASS as iconClass } from './section-props'

type IndexState = FolderSizeIndexStatus['state']

const FILES_COUNTED_LATER = 'files are counted once indexing finishes'

const REBUILDABLE_STATES: ReadonlySet<IndexState> = new Set(['upToDate', 'updating', 'paused'])

function useIndexStorage(state: IndexState) {
  const exclusions = useConfigStore((config) => config.indexExclusions)
  const [storage, setStorage] = useState<FolderSizeIndexStorage | null>(null)

  useEffect(() => {
    let cancelled = false
    void loadIndexStorage().then((loaded) => {
      if (!cancelled) {
        setStorage(loaded)
      }
    })
    return () => {
      cancelled = true
    }
  }, [state, exclusions])

  return storage
}

function folderSummary(storage: FolderSizeIndexStorage): string[] {
  if (storage.folderCount === undefined) {
    return []
  }
  return [countLabel(storage.folderCount, 'folder')]
}

function fileSummary(storage: FolderSizeIndexStorage): string[] {
  if (storage.fileCount !== undefined) {
    return [countLabel(storage.fileCount, 'file')]
  }
  if (storage.folderCount !== undefined) {
    return [FILES_COUNTED_LATER]
  }
  return []
}

function contentsSummary(storage: FolderSizeIndexStorage) {
  const parts = [formatBytes(storage.sizeBytes), ...folderSummary(storage), ...fileSummary(storage)]
  return parts.join(DETAIL_SEPARATOR)
}

function openInNewTab(directory: string) {
  useSettingsStore.getState().close()
  openTabInActivePane(directory)
}

function IndexFilesRow({ storage }: { storage: FolderSizeIndexStorage }) {
  return (
    <SettingRow
      icon={<HardDriveIcon className={iconClass} />}
      title="Index files"
      description={
        <>
          <span className="block break-all">{storage.directory}</span>
          <span className="block">{contentsSummary(storage)}</span>
        </>
      }
      control={
        <Button onClick={() => openInNewTab(storage.directory)}>
          <FolderOpenIcon className="size-4" />
          Open in new tab
        </Button>
      }
    />
  )
}

function RebuildRow({ state }: { state: IndexState }) {
  const canRebuild = REBUILDABLE_STATES.has(state)
  return (
    <SettingRow
      icon={<RotateCcwIcon className={iconClass} />}
      title="Rebuild index"
      description="Rescans the whole disk in the background. Search and folder sizes keep working while it runs."
      control={
        <Button disabled={!canRebuild} onClick={() => void rebuildIndex()}>
          Rebuild
        </Button>
      }
    />
  )
}

export function IndexStorageRows({ state }: { state: IndexState }) {
  const storage = useIndexStorage(state)
  return (
    <>
      <SectionLabel>Index data</SectionLabel>
      {storage ? <IndexFilesRow storage={storage} /> : null}
      <RebuildRow state={state} />
    </>
  )
}
