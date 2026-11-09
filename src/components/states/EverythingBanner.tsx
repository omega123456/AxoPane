import { InfoBar } from '@/components/shell/InfoBar'
import { isWindowsPlatform } from '@/lib/keymap'
import type { FolderSizeIndexStatus } from '@/lib/types/ipc'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { FOCUS_RING } from '@/lib/focus-ring'

function isEverythingUnavailable(status: FolderSizeIndexStatus | null) {
  return status?.backend === 'everything' && !status.isAvailable
}

export function EverythingBanner() {
  const status = usePanesStore((state) => state.folderSizeIndexStatus)
  const dismissed = useConfigStore((state) => state.dismissedEverythingBanner)
  const dismiss = useConfigStore((state) => state.dismissEverythingBanner)

  if (!isWindowsPlatform()) {
    return null
  }
  if (dismissed) {
    return null
  }
  if (!isEverythingUnavailable(status)) {
    return null
  }

  return (
    <InfoBar
      severity="warning"
      label="Everything unavailable"
      title="Folder sizes unavailable"
      action={
        <a
          href="https://www.voidtools.com/downloads/"
          target="_blank"
          rel="noreferrer"
          className={`inline-flex h-8 items-center rounded-control border border-light-control-stroke border-b-light-control-stroke-bottom bg-light-control px-3 text-row text-light-text hover:bg-light-control-hover dark:border-dark-control-stroke dark:border-b-dark-control-stroke-bottom dark:bg-dark-control dark:text-dark-text dark:hover:bg-dark-control-hover ${FOCUS_RING}`}
        >
          Download
        </a>
      }
      onDismiss={dismiss}
      dismissLabel="Dismiss Everything banner"
    >
      Everything is not running.
    </InfoBar>
  )
}
