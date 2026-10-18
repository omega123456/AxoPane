import { InfoBar } from '@/components/shell/InfoBar'
import { detectPlatformOs } from '@/lib/keymap'
import { useConfigStore } from '@/stores/config-store'
import { usePanesStore } from '@/stores/panes-store'
import { FOCUS_RING } from '@/lib/focus-ring'

export function EverythingBanner() {
  const status = usePanesStore((state) => state.folderSizeIndexStatus)
  const dismissed = useConfigStore((state) => state.dismissedEverythingBanner)
  const dismiss = useConfigStore((state) => state.dismissEverythingBanner)

  if (
    detectPlatformOs() !== 'windows' ||
    dismissed ||
    status?.backend !== 'everything' ||
    status.isAvailable
  ) {
    return null
  }

  return (
    <InfoBar
      severity="warning"
      label="Everything unavailable"
      title="Folder sizes unavailable"
      action={
        // A link, not a button: it opens the download page in the browser.
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
